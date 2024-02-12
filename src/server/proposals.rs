use std::collections::BTreeMap;
use namada_sdk::governance::storage::proposal::StorageProposal;
use namada_sdk::governance::utils::{ProposalResult, TallyResult, TallyType, VotePower};
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

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ResultResponse {
  /// The result of a proposal
  pub result: String,
  /// The type of tally required for this proposal
  pub tally_type: String,
  /// The total voting power during the proposal tally
  pub total_voting_power: VotePower,
  /// The total voting power from yay votes
  pub total_yay_power: VotePower,
  /// The total voting power from nay votes
  pub total_nay_power: VotePower,
  /// The total voting power from abstained votes
  pub total_abstain_power: VotePower,
}

impl From<ProposalResult> for ResultResponse {
  fn from(value: ProposalResult) -> Self {

    let result = match value.result {
      TallyResult::Passed => "passed".to_string(),
      TallyResult::Rejected => "rejected".to_string(),
    };

    let tally_type = match value.tally_type {
      TallyType::LessOneHalfOverOneThirdNay => "LessOneHalfOverOneThirdNay".to_string(),
      TallyType::TwoThirds => "TwoThirds".to_string(),
      TallyType::OneHalfOverOneThird => "OneHalfOverOneThird".to_string(),
    };

    ResultResponse {
      result,
      tally_type,
      total_voting_power: value.total_voting_power,
      total_yay_power: value.total_yay_power,
      total_nay_power: value.total_nay_power,
      total_abstain_power: value.total_abstain_power,
    }
  }
}
