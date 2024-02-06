use serde::{Deserialize, Serialize};
use namada_sdk::{proof_of_stake::types::ValidatorMetaData, types::{
  token::Amount,
  address::Address,
  dec::Dec,
}};

use super::endpoints::validator::UptimeValue;

/// Info for a single validator
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ValidatorInfo {
  pub nam_address: Address,
  pub tm_address: String,
  pub metadata: Option<ValidatorMetaData>,
  pub stake: Amount,
  pub commission: Option<CommissionInfo>,
  pub state: String,
  pub uptime: UptimeValue,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ValidatorInfoShort {
  pub nam_address: Address,
  pub tm_address: String,
  pub stake: Amount,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CommissionInfo {
  pub commission_rate: Dec,
  pub max_commission_change_per_epoch: Dec,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ValidatorSet {
  pub consensus_count: u64,
  pub below_capacity_count: u64,
  pub consensus_set: Vec<ValidatorInfo>,
}
