use axum::{
  extract::{State, Path},
  Json,
};
use futures::future::try_join_all;
use tracing::info;
use crate::{
  server::ServerState,
  Error,
};

use namada_sdk::governance::storage::keys as governance_storage;
use namada_sdk::rpc::{query_storage_value, query_proposal_by_id};
use crate::server::proposals::{ProposalInfo, ProposalList};

pub async fn get_all_proposals(
  State(state): State<ServerState>,
) -> Result<Json<Option<ProposalList>>, Error> {
  info!("calling /proposals/list");

  let last_proposal_id_key = governance_storage::get_counter_key();
  let last_proposal_id: u64 = query_storage_value(&state.http_client, &last_proposal_id_key).await?;

  let mut tasks = Vec::new();

  // Spawn a task for each entry in consensus set
  for id in 0..last_proposal_id {
      let task = fetch_proposal_info(&state, id);
      tasks.push(task);
  }

  // Collect results of all tasks concurrently
  let all_proposals: Vec<Option<ProposalInfo>> = try_join_all(tasks).await?;
  // Remove any 'None' values that might have been returned when querying a proposal id
  let valid_proposals: Vec<ProposalInfo> = all_proposals.into_iter()
    .filter_map(|opt_proposal| opt_proposal)
    .collect();

  let proposal_list = ProposalList {
    proposals: valid_proposals,
  };

  Ok(Json(Some(proposal_list)))
}

pub async fn get_proposal(
  State(state): State<ServerState>,
  Path(id): Path<u64>,
) -> Result<Json<Option<ProposalInfo>>, Error> {
  info!("calling /proposals/:id/info");

  if let Some(proposal) = query_proposal_by_id(&state.http_client, id).await? {
    return Ok(Json(Some(ProposalInfo::from(proposal))))
  }

  return Ok(Json(None))
}

async fn fetch_proposal_info(state: &ServerState, id: u64) -> Result<Option<ProposalInfo>, Error> {
  if let Some(proposal) = query_proposal_by_id(&state.http_client, id).await? {
    return Ok(Some(ProposalInfo::from(proposal)))
  }
  else {
    return Ok(None)
  }
}