use std::str::FromStr;

use axum::{
    extract::{Path, State},
    Json,
};
use tendermint_rpc::HttpClient;
use tracing::info;
use crate::{
    server::{account::{AccountSummary, AccountUpdates}, ServerState},
    Error,
};
use tokio;
use namada_sdk::{queries::RPC, rpc::{get_token_balance, is_steward, is_validator, known_address, query_native_token, query_storage_value}, types::{address::Address, token::Amount}};
use namada_sdk::types::storage::Key;
use sqlx::Row as TRow;

/// Retrieves the update history for a specific account.
///
/// This function handles a web request that queries the update history of a specified account.
/// It returns the updates in JSON format, with each field representing a different aspect
/// of the account that has been updated. The updates are returned in an ordered manner for each field.
///
/// # Arguments
///
/// * `account_id`: - The identifier of the account. This is extracted from the URL path
///   as a path parameter.
///
/// # Returns
///
/// On success, returns a JSON representation of the
///   account's update history. If no updates are found for the given account, `None` is returned.
///   On failure, returns an `Error`.
///
/// # Example
///
/// ```no_run
/// // Assuming the function is part of a route handler in a web application:
/// // GET /account/updates/{account_id}
/// // Where {account_id} is a dynamic path parameter(Address formatted as an string) representing the account ID.
/// ```
///
/// # Errors
///
/// This function may return errors related to database access, data serialization, or other
/// issues encountered during the processing of the request.
pub async fn get_account_updates(
    State(state): State<ServerState>,
    Path(account_id): Path<String>,
) -> Result<Json<Option<AccountUpdates>>, Error> {
    let Some(thresholds) = state.db.account_thresholds(&account_id).await? else {
        // account_id does not exists
        return Ok(Json(None));
    };

    let Some(hashes) = state.db.account_vp_codes(&account_id).await? else {
        // account_id does not exists
        return Ok(Json(None));
    };

    // if there are not thresholds updates associated with this accound_id
    // our query will return an empty lists, thanks to the usage of the
    // COALESCE operator
    let thresholds = thresholds
        .try_get::<Vec<i32>, _>("thresholds")?
        .into_iter()
        .map(|v| v as u8)
        .collect();

    // If there are not vp_code_hashes updates associated to this account_id
    // our query function will return an empty lists.
    let code_hashes = hashes
        .try_get::<Vec<Vec<u8>>, _>("code_hashes")?
        .into_iter()
        .map(hex::encode)
        .collect();

    let public_keys_result = state.db.account_public_keys(&account_id).await?;

    // Add public_keys to the combined row
    let public_keys = public_keys_result
        .into_iter()
        .filter_map(|r| r.try_get::<Vec<String>, _>("public_keys_batch").ok())
        .collect();

    Ok(Json(Some(AccountUpdates {
        account_id,
        thresholds,
        code_hashes,
        public_keys,
    })))
}

pub async fn get_account_summary(
    State(state): State<ServerState>,
    Path(address): Path<String>,
  ) -> Result<Json<Option<AccountSummary>>, Error> {
    info!("calling /account/:address/info");

    let nam_address = match Address::from_str(&address) {
        Ok(result) => result,
        Err(_) => return Err(Error::InvalidValidatorAddress)
    };

    let native_token = query_native_token(&state.http_client).await?;

    let is_steward = is_steward(&state.http_client, &nam_address).await;
    // let is_validator = is_validator(&state.http_client, &nam_address).await?;
    // let known_address = known_address(&state.http_client, &nam_address).await?;
    // let native_balance = get_token_balance(&state.http_client, &native_token, &nam_address).await?;

    let (is_validator, known_address, native_balance): (bool, bool, Amount) = tokio::try_join!(
        is_validator(&state.http_client, &nam_address),
        known_address(&state.http_client, &nam_address),
        get_token_balance(&state.http_client, &native_token, &nam_address),
    )?;

    // TODO: balances for other tokens on chain

    // use async pattern
    // query ibc tokens and balances
    // let ibc_prefixes = vec![ibc_denom_key_prefix(None)];

    // for prefix in prefixes {

    // }

    // let tokens = query_tokens(context, None, Some(&owner)).await;
    // for (token_alias, token) in tokens {
    //     let balance =
    //         get_token_balance(context.client(), &token, &owner).await;
    //     if !balance.is_zero() {
    //         let balance = context.format_amount(&token, balance).await;
    //         display_line!(context.io(), "{}: {}", token_alias, balance);
    //     }
    // }

    let account_summary = AccountSummary {
        is_validator,
        is_steward,
        native_balance,
        known_address,
    };
  
    return Ok(Json(Some(account_summary)))
  }

//   async fn query_token_prefix(client: &HttpClient, key: &Key) {
//     let values = convert_response(
//         RPC.shell()
//             .storage_prefix(client, None, None, false, key)
//             .await,
//     )?;
//   }