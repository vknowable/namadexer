use axum::{
    extract::{Path, Query, State},
    Json,
};
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow as Row;
use sqlx::Row as TRow;
use std::{collections::{BTreeSet, HashMap}, str::FromStr};
use tracing::{info, instrument};
use namada_sdk::{masp_primitives::consensus, proof_of_stake::types::{ValidatorState, WeightedValidator}, state::Epoch, types::address::Address};
use namada_sdk::rpc::{get_validator_stake, get_validator_state, query_epoch, query_metadata};
use namada_sdk::queries::RPC;

use crate::{server::{validators::{CommissionInfo, ValidatorSet, ValidatorInfoShort}, ServerState}, Error};
use crate::server::ValidatorInfo;

// Retrieve the count of commit for a range of blocks from the sql query result.
#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
#[repr(transparent)]
struct CommitCount(pub i64);

impl TryFrom<&Row> for CommitCount {
    type Error = Error;

    #[instrument(level = "trace", skip(row))]
    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let count: i64 = row.try_get("count")?;

        Ok(CommitCount(count))
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct UptimeValue {
    pub uptime: f64,
}

pub async fn get_validator_uptime(
    State(state): State<ServerState>,
    Path(validator_address): Path<String>,
    Query(params): Query<HashMap<String, i32>>,
) -> Result<Json<UptimeValue>, Error> {
    info!("calling /validator/:validator_address/uptime");

    let start = params.get("start");
    let end = params.get("end");

    let va = hex::decode(validator_address)?;
    let row = state.db.validator_uptime(&va, start, end).await?;
    let cc = CommitCount::try_from(&row)?;

    // default range is 500 blocks
    let mut ranger_size: f64 = 500.0;

    if let (Some(s), Some(e)) = (start, end) {
        ranger_size = (e - s).into();
    }

    let uv = UptimeValue {
        uptime: (cc.0 as f64) / ranger_size,
    };

    Ok(Json(uv))
}

/// caculate uptime from validator tendermint address
pub async fn calculate_uptime(state: &ServerState, validator_address: &String) -> Result<UptimeValue, Error>{

    let va = hex::decode(validator_address)?;
    let row = state.db.validator_uptime(&va, None, None).await?;
    let cc = CommitCount::try_from(&row)?;

    // default range is 500 blocks
    let ranger_size: f64 = 500.0;

    let uv = UptimeValue {
        uptime: (cc.0 as f64) / ranger_size,
    };

    Ok(uv)
}

/// get info for a single validator by tendermint or tnam address
pub async fn get_validator_info(
    State(state): State<ServerState>,
    Path(validator_address): Path<String>,
) -> Result<Json<ValidatorInfo>, Error> {
    info!("calling /validator/:validator_address/info");

    // TODO: improve this code
    let mut nam_address: Address;
    let mut tm_address: String;
    match validator_address.starts_with("tnam") {
        true => {
            nam_address = match Address::from_str(&validator_address) {
                Ok(result) => result,
                Err(_) => return Err(Error::InvalidValidatorAddress)
            };
            tm_address = if let Some(consensus_key) = unwrap_client_response::<tendermint_rpc::HttpClient, _>(RPC.vp().pos().consensus_key(&state.http_client, &nam_address).await) {
                namada_sdk::types::key::tm_consensus_key_raw_hash(&consensus_key)
            } else {
                "".to_string()
            };
        },
        false => {
            tm_address = validator_address.to_ascii_uppercase();
            nam_address = match unwrap_client_response::<tendermint_rpc::HttpClient, _>(RPC.vp().pos().validator_by_tm_addr(&state.http_client, &tm_address).await) {
                Some(address) => address,
                None => return Err(Error::InvalidValidatorAddress)
            };
        }
    }

    // let tm_address = validator_address.to_ascii_uppercase();
    // let nam_address = match unwrap_client_response::<tendermint_rpc::HttpClient, _>(RPC.vp().pos().validator_by_tm_addr(&state.http_client, &tm_address).await) {
    //     Some(address) => address,
    //     None => return Err(Error::InvalidValidatorAddress)
    // };

    let epoch = query_epoch(&state.http_client).await?;
    let ((metadata, commission_pair), stake, state_enum) = tokio::try_join!(
        query_metadata(&state.http_client, &nam_address, Some(epoch)),
        get_validator_stake(&state.http_client, epoch, &nam_address),
        get_validator_state(&state.http_client, &nam_address, Some(epoch)),
    )?;

    let val_state = match state_enum {
        Some(ValidatorState::Consensus) => "consensus".to_string(),
        Some(ValidatorState::BelowCapacity) => "below_capacity".to_string(),
        Some(ValidatorState::BelowThreshold) => "below_threshold".to_string(),
        Some(ValidatorState::Inactive) => "inactive".to_string(),
        Some(ValidatorState::Jailed) => "jailed".to_string(),
        _ => "unknown".to_string(),
    };

    let commission = match commission_pair {
        Some(pair) => Some(CommissionInfo {commission_rate: pair.commission_rate, max_commission_change_per_epoch: pair.max_commission_change_per_epoch}),
        None => None
    };

    let uptime = calculate_uptime(&state, &tm_address).await?;

    let validator_info = ValidatorInfo {
        nam_address,
        tm_address,
        metadata,
        stake,
        commission,
        state: val_state,
        uptime,
    };

    Ok(Json(validator_info))
}

/// get validator sets by category
pub async fn get_validator_set(
    State(state): State<ServerState>,
) -> Result<Json<ValidatorSet>, Error> {
    info!("calling /validator/set");

    // let epoch = query_epoch(&state.http_client).await?;

    let consensus: BTreeSet<WeightedValidator> =
        unwrap_client_response::<tendermint_rpc::HttpClient, _>(
            RPC.vp()
                .pos()
                .consensus_validator_set(&state.http_client, &None)
                .await,
        );

    let mut tasks = Vec::new();

    // Spawn a task for each entry in consensus set
    for entry in consensus.iter() {
        let task = fetch_entry_info(&state, &entry);
        tasks.push(task);
    }

    // Collect results of all tasks concurrently
    let consensus_set = try_join_all(tasks).await?;

    // TODO: similar operation for below_capacity needed
    // might be better to decouple these
    let below_capacity: BTreeSet<WeightedValidator> =
        unwrap_client_response::<tendermint_rpc::HttpClient, _>(
            RPC.vp()
                .pos()
                .below_capacity_validator_set(&state.http_client,&None)
                .await,
    );

    let validator_count = ValidatorSet {
        consensus_count: consensus_set.len() as u64,
        below_capacity_count: below_capacity.len() as u64,
        consensus_set,
    };

    Ok(Json(validator_count))
}

async fn fetch_entry_info(state: &ServerState, entry: &WeightedValidator) -> Result<ValidatorInfo, Error> {
    let nam_address = &entry.address;
    let tm_address = if let Some(consensus_key) = unwrap_client_response::<tendermint_rpc::HttpClient, _>(RPC.vp().pos().consensus_key(&state.http_client, &nam_address).await) {
        namada_sdk::types::key::tm_consensus_key_raw_hash(&consensus_key)
    } else {
        "".to_string()
    };

    // TODO: refactor this repeated code
    let ((metadata, commission_pair), state_enum) = tokio::try_join!(
        query_metadata(&state.http_client, &nam_address, None),
        get_validator_state(&state.http_client, &nam_address, None),
    )?;

    let commission = match commission_pair {
        Some(pair) => Some(CommissionInfo {commission_rate: pair.commission_rate, max_commission_change_per_epoch: pair.max_commission_change_per_epoch}),
        None => None
    };

    let val_state = match state_enum {
        Some(ValidatorState::Consensus) => "consensus".to_string(),
        Some(ValidatorState::BelowCapacity) => "below_capacity".to_string(),
        Some(ValidatorState::BelowThreshold) => "below_threshold".to_string(),
        Some(ValidatorState::Inactive) => "inactive".to_string(),
        Some(ValidatorState::Jailed) => "jailed".to_string(),
        _ => "unknown".to_string(),
    };

    let uptime = calculate_uptime(&state, &tm_address).await?;

    Ok(ValidatorInfo {
        nam_address: nam_address.clone(),
        tm_address,
        stake: entry.bonded_stake,
        metadata,
        commission,
        state: val_state,
        uptime,
    })
}

// pub async fn process_consensus_set(State(state): State<ServerState>, consensus: BTreeSet<ValidatorInfoShort>) -> Result<Vec<ValidatorInfoShort>, Error> {
//     let mut tasks = Vec::new();

//     // Spawn a task for each entry in consensus set
//     for entry in consensus.iter() {
//         let state_clone = state.clone(); // Clone state for each task
//         let entry_clone = entry.clone(); // Clone entry for each task
//         let task = fetch_entry_info(&state_clone, &entry_clone);
//         tasks.push(task);
//     }

//     // Collect results of all tasks concurrently
//     let results = try_join_all(tasks).await?;

//     Ok(results)
// }

// helper function for querying POS module directly
fn unwrap_client_response<C: namada_sdk::queries::Client, T>(
    response: Result<T, C::Error>,
) -> T {
    response.unwrap_or_else(|err| {
        panic!("Error in the query: {:?}", err.to_string());
    })
}