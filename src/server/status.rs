use serde::{Deserialize, Serialize};
use tendermint::{chain, block, time};
use namada_sdk::types::{
  token::Amount,
  address::Address,
};
/// Chain status/overview for the explorer dashboard (frontpage)
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ChainStatus {
    pub chain_id: chain::Id,
    pub latest_height: block::Height,
    pub last_block_time: time::Time,
    pub staking_info: StakingInfo,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct StakingInfo {
  pub active_validators: u64,
  pub total_validators: u64,
  pub bonded_supply: Amount,
  // pub unbonding_supply: Amount,
  pub total_supply: Amount,
  pub native_token: Address,
}
