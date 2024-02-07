use namada_sdk::governance::parameters::GovernanceParameters;
use namada_sdk::governance::pgf::parameters::PgfParameters;
use namada_sdk::state::Epoch;
use serde::{Deserialize, Serialize};
use tendermint::{chain, block, time};
use namada_sdk::types::{
  token::Amount,
  address::Address,
  dec::Dec,
};
use namada_sdk::proof_of_stake::parameters::{PosParams, OwnedPosParams};
use namada_sdk::types::time::DurationSecs;

/// Chain status/overview for the explorer dashboard (frontpage)
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ChainStatus {
    pub chain_id: chain::Id,
    pub latest_height: block::Height,
    pub last_block_time: time::Time,
    pub epoch: Epoch,
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

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ChainParams {
  pub protocol_params: ProtocolParams,
  pub pos_params: PosResponse,
  pub pgf_params: PgfParameters,
  pub gov_params: GovResponse,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct PosResponse {
  pub owned: OwnedPosResponse,
  pub max_proposal_period: u64,
}

impl From<PosParams> for PosResponse {
  fn from(value: PosParams) -> Self {
      PosResponse {
        owned: OwnedPosResponse::from(value.owned),
        max_proposal_period: value.max_proposal_period,
      }
  }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct OwnedPosResponse {
  pub max_validator_slots: u64,
  pub pipeline_len: u64,
  pub unbonding_len: u64,
  pub tm_votes_per_token: Dec,
  pub block_proposer_reward: Dec,
  pub block_vote_reward: Dec,
  pub max_inflation_rate: Dec,
  pub target_staked_ratio: Dec,
  pub duplicate_vote_min_slash_rate: Dec,
  pub light_client_attack_min_slash_rate: Dec,
  pub cubic_slashing_window_length: u64,
  pub validator_stake_threshold: Amount,
  pub liveness_window_check: u64,
  pub liveness_threshold: Dec,
  pub rewards_gain_p: Dec,
  pub rewards_gain_d: Dec,
}

impl From<OwnedPosParams> for OwnedPosResponse {
  fn from(value: OwnedPosParams) -> Self {
      OwnedPosResponse {
        max_validator_slots: value.max_validator_slots,
        pipeline_len: value.pipeline_len,
        unbonding_len: value.unbonding_len,
        tm_votes_per_token: value.tm_votes_per_token,
        block_proposer_reward: value.block_proposer_reward,
        block_vote_reward: value.block_vote_reward,
        max_inflation_rate: value.max_inflation_rate,
        target_staked_ratio: value.target_staked_ratio,
        duplicate_vote_min_slash_rate: value.duplicate_vote_min_slash_rate,
        light_client_attack_min_slash_rate: value.light_client_attack_min_slash_rate,
        cubic_slashing_window_length: value.cubic_slashing_window_length,
        validator_stake_threshold: value.validator_stake_threshold,
        liveness_window_check: value.liveness_window_check,
        liveness_threshold: value.liveness_threshold,
        rewards_gain_p: value.rewards_gain_p,
        rewards_gain_d: value.rewards_gain_d,
      }
  }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct GovResponse {
  pub min_proposal_fund: Amount,
  pub max_proposal_code_size: u64,
  pub min_proposal_voting_period: u64,
  pub max_proposal_period: u64,
  pub max_proposal_content_size: u64,
  pub min_proposal_grace_epochs: u64,
}

impl From<GovernanceParameters> for GovResponse {
  fn from(value: GovernanceParameters) -> Self {
      GovResponse {
        min_proposal_fund: value.min_proposal_fund,
        max_proposal_code_size: value.max_proposal_code_size,
        min_proposal_voting_period: value.min_proposal_voting_period,
        max_proposal_period: value.min_proposal_voting_period,
        max_proposal_content_size: value.max_proposal_content_size,
        min_proposal_grace_epochs: value.min_proposal_grace_epochs,
      }
  }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ProtocolParams {
  pub min_epoch_dur: DurationSecs,
  pub min_blocks_epoch: u64,
  pub max_block_dur: u64,
  pub tx_allowlist: Vec<String>,
  pub vp_allowlist: Vec<String>,
  pub max_block_gas: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct EpochResponse {
  pub epoch: Epoch,
}
