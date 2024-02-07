use std::collections::BTreeMap;
use namada_sdk::governance::storage::proposal::StorageProposal;
use namada_sdk::governance::ProposalType;
use namada_sdk::types::{
  storage::Epoch,
  address::Address,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ProposalInfo {
    /// The proposal id
    pub id: u64,
    /// The proposal content
    pub content: BTreeMap<String, String>,
    /// The proposal author address
    pub author: Address,
    /// The proposal type
    pub r#type: ProposalType,
    /// The epoch from which voting is allowed
    pub voting_start_epoch: Epoch,
    /// The epoch from which voting is stopped
    pub voting_end_epoch: Epoch,
    /// The epoch from which this changes are executed
    pub grace_epoch: Epoch,
}

impl From<StorageProposal> for ProposalInfo {
  fn from(value: StorageProposal) -> Self {
      ProposalInfo {
        id: value.id,
        content: value.content,
        author: value.author,
        r#type: value.r#type,
        voting_start_epoch: value.voting_start_epoch,
        voting_end_epoch: value.voting_end_epoch,
        grace_epoch: value.grace_epoch,
      }
  }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ProposalList {
  pub proposals: Vec<ProposalInfo>,
}