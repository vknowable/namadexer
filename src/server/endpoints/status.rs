use std::ops::Add;
use std::str::FromStr;

use axum::{
  extract::State,
  Json,
};
use namada_sdk::proof_of_stake::storage::{liveness_missed_votes_handle, liveness_sum_missed_votes_handle};
use namada_sdk::proof_of_stake::types::LivenessSumMissedVotes;
use namada_sdk::storage::collections::LazyCollection;
use tendermint_rpc::{Paging, PerPage, PageNumber, Client};
use tokio;
use tracing::info;
use namada_sdk::rpc::{query_storage_value, query_storage_prefix, query_epoch, query_native_token, get_token_total_supply, get_total_staked_tokens, get_all_validators, get_pos_params, query_governance_parameters, query_pgf_parameters};
use namada_sdk::types::{
  storage::Epoch,
  address::Address,
};
use namada_parameters::{storage as param_storage, EpochDuration};

use crate::{
  server::ServerState,
  Error, BlockInfo,
};
use crate::server::status::{ChainParams, ChainStatus, EpochResponse, GovResponse, PosResponse, ProtocolParams, StakingInfo};

pub async fn get_status(
  State(state): State<ServerState>,
) -> Result<Json<Option<ChainStatus>>, Error> {
  info!("calling /chain/status");

  // get the last block
  let row = state.db.get_last_block().await?;
  let block = BlockInfo::try_from(&row)?;

  let chain_id = block.header.chain_id;
  let latest_height = block.header.height;
  let last_block_time = block.header.time;

  let epoch: Epoch = query_epoch(&state.http_client).await?;
  // paging parameters for the tendermint validators rpc query
  let paging = Paging::Specific { page_number: PageNumber::from(1), per_page: PerPage::from(1) };

  let native_token: Address = query_native_token(&state.http_client).await?;
  let active_validators = state.http_client.validators(latest_height, paging).await?;
  let (bonded_supply, total_supply, validators) = tokio::try_join!(
    get_total_staked_tokens(&state.http_client, epoch),
    get_token_total_supply(&state.http_client, &native_token),
    get_all_validators(&state.http_client, epoch),
  )?;

  let chain_status = ChainStatus {
    chain_id,
    latest_height,
    last_block_time,
    epoch,
    staking_info: StakingInfo {
      active_validators: active_validators.total as u64,
      total_validators: validators.len() as u64,
      bonded_supply,
      total_supply,
      native_token,
    }
  };

  Ok(Json(Some(chain_status)))
}

pub async fn get_chain_params(
  State(state): State<ServerState>,
) -> Result<Json<Option<ChainParams>>, Error> {
  info!("calling /chain/params");

  let pos_params = get_pos_params(&state.http_client).await?;
  let pgf_params = query_pgf_parameters(&state.http_client).await;
  let gov_params = query_governance_parameters(&state.http_client).await;

  // protocol params have to be queried individually
  let epoch_dur_key = param_storage::get_epoch_duration_storage_key();
  let max_block_dur_key = param_storage::get_max_expected_time_per_block_key();
  let tx_allowlist_key = param_storage::get_tx_allowlist_storage_key();
  let vp_allowlist_key = param_storage::get_vp_allowlist_storage_key();
  let max_block_gas_key = &param_storage::get_max_block_gas_key();
  
  let (epoch_dur, max_block_dur, tx_allowlist, vp_allowlist, max_block_gas): 
    (EpochDuration, u64, Vec<String>, Vec<String>, u64) = tokio::try_join!(
      query_storage_value(&state.http_client, &epoch_dur_key),
      query_storage_value(&state.http_client, &max_block_dur_key),
      query_storage_value(&state.http_client, &tx_allowlist_key),
      query_storage_value(&state.http_client, &vp_allowlist_key),
      query_storage_value(&state.http_client, &max_block_gas_key),
  )?;


  let chain_params = ChainParams {
    protocol_params: ProtocolParams {
      min_epoch_dur: epoch_dur.min_duration,
      min_blocks_epoch: epoch_dur.min_num_of_blocks,
      max_block_dur,
      tx_allowlist,
      vp_allowlist,
      max_block_gas,
    },
    pos_params: PosResponse::from(pos_params),
    pgf_params,
    gov_params: GovResponse::from(gov_params),
  };

  Ok(Json(Some(chain_params)))
}

pub async fn get_last_epoch(
  State(state): State<ServerState>,
) -> Result<Json<Option<EpochResponse>>, Error> {
  info!("calling /chain/epoch/last");

  let epoch = query_epoch(&state.http_client).await?;

  Ok(Json(Some(EpochResponse { epoch, })))
}