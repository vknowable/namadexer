use namada_sdk::state::Epoch;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tracing::{instrument, trace};

use sqlx::postgres::PgRow as Row;
use sqlx::Row as TRow;
use tendermint::block::{Height, Round};
use tendermint::AppHash;
use tendermint::{
    Signature as TmSignature,
    account::Id as AccountId,
    block::header::Version,
    block::{parts::Header as PartSetHeader, Header, Id as BlockId},
    chain::Id,
    Hash, Time,
};

use super::{from_hex, serialize_hex};
use crate::error::Error;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SignaturesInfo {
    pub block_id: String,
    pub signatures: Vec<Signature>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Signature {
  pub validator_address: Option<AccountId>,
}

impl TryFrom<&Row> for Signature {
  type Error = Error;

  #[instrument(level = "trace", skip(row))]
  fn try_from(row: &Row) -> Result<Self, Self::Error> {

      // block_id
      let block_id: Vec<u8> = row.try_get("block_id")?;
      trace!("parsed block_id {:?}", &block_id);

      // validator address
      let validator_address: Vec<u8> = row.try_get("validator_address")?;
      let validator_address = match AccountId::try_from(validator_address) {
        Ok(id) => Some(id),
        Err(_) => None,
      };
      trace!("parsed validator_address {:?}", validator_address);

      Ok(Signature {
          validator_address,
      })
  }
}