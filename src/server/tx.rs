use crate::error::Error;
use namada_sdk::ibc::primitives::proto::Any;
use serde::{Deserialize, Serialize};

use super::utils::serialize_optional_hex;

use sqlx::postgres::PgRow as Row;
use sqlx::Row as TRow;

// namada::ibc::applications::transfer::msgs::transfer::TYPE_URL has been made private and can't be access anymore
// const MSG_TRANSFER_TYPE_URL: &str = "/ibc.applications.transfer.v1.MsgTransfer";

// we have a variant for MsgTransfer, but there are other message types
// defined in https://github.com/cosmos/ibc-rs/blob/main/crates/ibc/src/core/msgs.rs
// however none of them implement serde traits, so lets use Any as the general
// abstraction for it. Any is serializeable
/// Defines the support IBC transactions.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum IbcTx {
    // we do not use MsgTransfer directly as it does not
    // implements serde traits.
    MsgTransfer(Any),
    Any(Any),
}

/// The relevant information regarding wrapper transactions and their types.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct WrapperTxInfo {
    /// The hash that idenfities this transaction
    #[serde(with = "hex::serde")]
    hash: Vec<u8>,
    /// The block this transaction belongs to.
    #[serde(with = "hex::serde")]
    block_id: Vec<u8>,
    /// The fee payer type encoded as a string
    fee_payer: Option<String>,
    /// The transaction fee only for tx_type Wrapper (otherwise empty)
    fee_amount_per_gas_unit: Option<String>,
    fee_token: Option<String>,
    /// Gas limit (only for Wrapper tx)
    gas_limit_multiplier: Option<i64>,
    atomic: Option<bool>,
    return_code: Option<i32>, // New field for return_code
}

impl TryFrom<Row> for WrapperTxInfo {
    type Error = Error;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let hash: Vec<u8> = row.try_get("hash")?;
        let block_id: Vec<u8> = row.try_get("block_id")?;
        let fee_payer: Option<String> = row.try_get("fee_payer")?;
        let fee_amount_per_gas_unit: Option<String> = row.try_get("fee_amount_per_gas_unit")?;
        let fee_token: Option<String> = row.try_get("fee_token")?;
        let gas_limit_multiplier: Option<i64> = row.try_get("gas_limit_multiplier")?;
        let atomic: Option<bool> = row.try_get("atomic")?;
        let return_code: Option<i32> = row.try_get("return_code")?;

        Ok(Self {
            hash,
            block_id,
            fee_payer,
            fee_amount_per_gas_unit,
            fee_token,
            gas_limit_multiplier,
            atomic,
            return_code, // Assigning return_code to the struct field
        })
    }
}

/// The relevant information regarding inner transactions and their types.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct InnerTxInfo {
    /// The hash that idenfities this transaction
    #[serde(with = "hex::serde")]
    hash: Vec<u8>,
    /// The block this transaction belongs to.
    #[serde(with = "hex::serde")]
    block_id: Vec<u8>,
    /// id for the wrapper tx if the tx is decrypted. otherwise it is null.
    #[serde(with = "hex::serde")]
    wrapper_id: Vec<u8>,
    /// The transaction fee only for tx_type Wrapper (otherwise empty)
    #[serde(serialize_with = "serialize_optional_hex")]
    code: Option<Vec<u8>>,
    code_type: Option<String>,
    #[serde(serialize_with = "serialize_optional_hex")]
    memo: Option<Vec<u8>>,
    data: Option<serde_json::Value>,
    return_code: Option<i32>, // New field for return_code
}

impl InnerTxInfo {
    pub fn data(&self) -> serde_json::Value {
        self.data.clone().unwrap_or_default()
    }
}

impl TryFrom<Row> for InnerTxInfo {
    type Error = Error;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let hash: Vec<u8> = row.try_get("hash")?;
        let block_id: Vec<u8> = row.try_get("block_id")?;
        let wrapper_id: Vec<u8> = row.try_get("wrapper_id")?;
        let code: Option<Vec<u8>> = row.try_get("code")?;
        let code_type: Option<String> = row.try_get("code_type")?;
        let memo: Option<Vec<u8>> = row.try_get("memo")?;
        let data: Option<serde_json::Value> = row.try_get("data")?;
        let return_code = row.try_get("return_code")?;

        Ok(Self {
            hash,
            block_id,
            wrapper_id,
            code,
            code_type,
            memo,
            data,
            return_code, // Assigning return_code to the struct field
        })
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct VoteProposalTx {
    pub id: i64,
    pub vote: String,
    pub voter: String,
    pub delegations: Vec<String>,
    #[serde(with = "hex::serde")]
    pub tx_id: Vec<u8>,
}

impl TryFrom<Row> for VoteProposalTx {
    type Error = Error;

    fn try_from(value: Row) -> Result<Self, Self::Error> {
        let id = value.try_get::<i64, _>("vote_proposal_id")?;

        let vote = value.try_get::<String, _>("vote")?;
        let voter = value.try_get::<String, _>("voter")?;
        let tx_id = value.try_get::<Vec<u8>, _>("tx_id")?;

        // empty this comes from another table.
        let delegations = vec![];

        Ok(Self {
            id,
            vote,
            voter,
            delegations,
            tx_id,
        })
    }
}

/// The relevant information regarding transfers from the tx_transfer view
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct TransferTxInfo {
    /// The hash that idenfities this transaction
    #[serde(with = "hex::serde")]
    txid: Vec<u8>,
    source: Option<String>,
    target: Option<String>,
    token: Option<String>,
    amount: Option<String>,
    shielded: Option<String>,
    #[serde(serialize_with = "serialize_optional_hex")]
    memo: Option<Vec<u8>>,
}

impl TryFrom<Row> for TransferTxInfo {
    type Error = Error;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let txid: Vec<u8> = row.try_get("txid")?;
        let source: Option<String> = row.try_get("source")?;
        let target: Option<String> = row.try_get("target")?;
        let token: Option<String> = row.try_get("token")?;
        let amount: Option<String> = row.try_get("amount")?;
        let shielded: Option<String> = row.try_get("shielded")?;
        let memo: Option<Vec<u8>> = row.try_get("memo")?;

        Ok(Self {
            txid,
            source,
            target,
            token,
            amount,
            shielded,
            memo,
        })
    }
}