use axum::{
  extract::State,
  Json,
};
use tendermint_rpc::{Paging, PerPage, PageNumber, Client};
use tokio;
use tracing::info;
use namada_sdk::rpc::{query_epoch, query_native_token, get_token_total_supply, get_total_staked_tokens, get_all_validators};
use namada_sdk::types::{
  storage::Epoch,
  address::Address,
};

use crate::{
  server::ServerState,
  Error, BlockInfo,
};
use crate::server::status::{ChainStatus, StakingInfo};

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