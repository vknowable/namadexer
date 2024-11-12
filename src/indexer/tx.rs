use serde_json::json;
use serde::{Serialize, Deserialize};
use std::str::FromStr;
use std::collections::{HashMap, BTreeMap};
use tracing::info;
use sqlx::{QueryBuilder, Transaction};
use namada_sdk::{
    key::common::PublicKey,
    account::{InitAccount, UpdateAccount},
    borsh::BorshDeserialize,
    governance::{InitProposalData, VoteProposalData},
    tx::data::{
        pgf::UpdateStewardCommission,
        pos::{
            BecomeValidator, Bond, CommissionChange, ConsensusKeyChange, MetaDataChange,
            Redelegation, Unbond, Withdraw,
        },
    },
    address::Address,
    eth_bridge_pool::PendingTransfer,
    token::{Account, DenominatedAmount, Transfer},
};
use namada_core::masp::MaspTxId;

use crate::error::Error;

const WRAPPER_TX_TABLE_NAME: &str = "wrapper_transactions";
const INNER_TX_TABLE_NAME: &str = "inner_transactions";

#[derive(Debug, Clone)]
pub struct AccountsMap(pub BTreeMap<Account, DenominatedAmount>);

impl Serialize for AccountsMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_seq(self.0.iter().map(|(k, v)| {
            HashMap::from([
                ("owner", k.owner.encode()),
                ("token", k.token.encode()),
                ("amount", v.to_string_precise()),
            ])
        }))
    }
}

impl<'de> Deserialize<'de> for AccountsMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        <Vec<BTreeMap<String, String>> as Deserialize>::deserialize(
            deserializer,
        )
        .map(|v| {
            AccountsMap(
                v.into_iter()
                    .map(|val| {
                        let owner =
                            val.get("owner").expect("Cannot find owner");
                        let token =
                            val.get("token").expect("Cannot find token");
                        let amount =
                            val.get("amount").expect("Cannot find amount");

                        (
                            Account {
                                owner: Address::decode(owner)
                                    .expect("Cannot parse Address for owner"),
                                token: Address::decode(token)
                                    .expect("Cannot parse Address for token"),
                            },
                            DenominatedAmount::from_str(amount)
                                .expect("Cannot parse DenominatedAmount"),
                        )
                    })
                    .collect(),
            )
        })
    }
}

// Wrapper struct for serializing Transparent Transfer
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransparentTransfer {
    sources: AccountsMap,
    targets: AccountsMap,
    shielded_section_hash: Option<MaspTxId>,
}

impl TransparentTransfer {
    pub fn from_transfer(transfer: Transfer) -> Result<Self, Error> {
        let sources = AccountsMap(transfer.sources);
        let targets = AccountsMap(transfer.targets);
        
        Ok(TransparentTransfer {
            sources,
            targets,
            shielded_section_hash: transfer.shielded_section_hash,
        })
    }
}

pub fn decode_tx(type_tx: &String, data: &Vec<u8>) -> Result<serde_json::Value, Error> {
    // decode tx based on type string
    match type_tx.as_str() {
        "tx_transfer" => {
            let transfer = Transfer::try_from_slice(&data[..])?;
            let transfer_json = TransparentTransfer::from_transfer(transfer)?;
            Ok(serde_json::to_value(transfer_json)?)
        }
        "tx_bond" => {
            let bond = Bond::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(bond)?)
        }
        "tx_unbond" => {
            let unbond = Unbond::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(unbond)?)
        }
        // this is an ethereum transaction
        "tx_bridge_pool" => {
            // Only TransferToEthereum type is supported at the moment by namada and us.
            let tx_bridge = PendingTransfer::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_bridge)?)
        }
        "tx_vote_proposal" => {
            let tx_vote_proposal = VoteProposalData::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_vote_proposal)?)
        }
        "tx_reveal_pk" => {
            // nothing to do here, only check that data is a valid publicKey
            // otherwise this transaction must not make it into
            // the database.
            let tx_reveal_pk = PublicKey::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_reveal_pk)?)
        }
        "tx_resign_steward" => {
            // Not much to do, just, check that the address this transactions
            // holds in the data field is correct, or at least parsed succesfully.
            let tx_resign_steward = Address::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_resign_steward)?)
        }
        "tx_update_steward_commission" => {
            // Not much to do, just, check that the address this transactions
            // holds in the data field is correct, or at least parsed succesfully.
            let tx_update_steward_commission =
                UpdateStewardCommission::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_update_steward_commission)?)
        }
        "tx_init_account" => {
            // check that transaction can be parsed
            // before inserting it into database.
            // later accounts could be updated using
            // tx_update_account, however there is not way
            // so far to link those transactions to this.
            let tx_init_account = InitAccount::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_init_account)?)
        }
        "tx_update_account" => {
            // check that transaction can be parsed
            // before storing it into database
            let tx_update_account = UpdateAccount::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_update_account)?)
        }
        "tx_ibc" => {
            info!("we do not handle ibc transaction yet");
            Ok(serde_json::to_value(hex::encode(&data[..]))?)
        }
        "tx_become_validator" => {
            let tx_become_validator = BecomeValidator::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_become_validator)?)
        }
        "tx_change_consensus_key" => {
            let tx_change_consensus_key =
                ConsensusKeyChange::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_change_consensus_key)?)
        }
        "tx_change_validator_commission" => {
            let tx_change_validator_commission =
                CommissionChange::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_change_validator_commission)?)
        }
        "tx_change_validator_metadata" => {
            let tx_change_validator_metadata =
                MetaDataChange::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_change_validator_metadata)?)
        }
        "tx_claim_rewards" => {
            let tx_claim_rewards = Withdraw::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_claim_rewards)?)
        }
        "tx_deactivate_validator" => {
            let tx_deactivate_validator = Address::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_deactivate_validator)?)
        }
        "tx_init_proposal" => {
            let tx_init_proposal = InitProposalData::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_init_proposal)?)
        }
        "tx_reactivate_validator" => {
            let tx_reactivate_validator = Address::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_reactivate_validator)?)
        }
        "tx_unjail_validator" => {
            let tx_unjail_validator = Address::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_unjail_validator)?)
        }
        "tx_redelegate" => {
            let tx_redelegate = Redelegation::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_redelegate)?)
        }
        "tx_withdraw" => {
            let tx_withdraw = Withdraw::try_from_slice(&data[..])?;
            Ok(serde_json::to_value(tx_withdraw)?)
        }
        _ => Ok(json!(null)),
    }
}

pub async fn insert_wrapper_txs<'a>(
    wrapper_txs: &Vec<WrapperTxDB>,
    sqlx_tx: &mut Transaction<'a, sqlx::Postgres>,
    network: &str,
) -> Result<(), Error> {

    let mut query_builder: QueryBuilder<_> = QueryBuilder::new(format!(
        "INSERT INTO {}.{WRAPPER_TX_TABLE_NAME}(
                hash, 
                block_id,
                fee_payer,
                fee_amount_per_gas_unit,
                fee_token,
                gas_limit_multiplier,
                atomic,
                return_code
            )",
        network
    ));

    let res = query_builder
    .push_values(
        wrapper_txs.into_iter(),
        |mut b,
            wrapper_tx_db| {
            b.push_bind(&wrapper_tx_db.hash)
                .push_bind(&wrapper_tx_db.block_id)
                .push_bind(&wrapper_tx_db.fee_payer)
                .push_bind(&wrapper_tx_db.fee_amount_per_gas_unit)
                .push_bind(&wrapper_tx_db.fee_token)
                .push_bind(&wrapper_tx_db.fee_gas_limit_multiplier)
                .push_bind(&wrapper_tx_db.atomic)
                .push_bind(&wrapper_tx_db.return_code);
        },
    )
    .build()
    .execute(&mut *sqlx_tx)
    .await
    .map(|_| ())
    .map_err(Error::from);

    res
}

pub async fn insert_inner_txs<'a>(
    inner_txs: &Vec<InnerTxDB>,
    sqlx_tx: &mut Transaction<'a, sqlx::Postgres>,
    network: &str
) -> Result<(), Error> {

    let mut query_builder: QueryBuilder<_> = QueryBuilder::new(format!(
        "INSERT INTO {}.{INNER_TX_TABLE_NAME}(
                hash, 
                block_id,
                wrapper_id,
                code,
                code_type,
                memo,
                data,
                return_code
            )",
        network
    ));

    let res = query_builder
    .push_values(
        inner_txs.into_iter(),
        |mut b,
            inner_tx_db| {
            b.push_bind(&inner_tx_db.hash)
                .push_bind(&inner_tx_db.block_id)
                .push_bind(&inner_tx_db.wrapper_id)
                .push_bind(&inner_tx_db.code)
                .push_bind(&inner_tx_db.code_type)
                .push_bind(&inner_tx_db.memo)
                .push_bind(&inner_tx_db.data)
                .push_bind(&inner_tx_db.return_code);
        },
    )
    .build()
    .execute(&mut *sqlx_tx)
    .await
    .map(|_| ())
    .map_err(Error::from);

    res
}

pub struct WrapperTxDB {
    pub hash: Vec<u8>,
    pub block_id: Vec<u8>,
    pub fee_payer: Option<String>,
    pub fee_amount_per_gas_unit: Option<String>,
    pub fee_token: Option<String>,
    pub fee_gas_limit_multiplier: Option<i64>,
    pub atomic: bool,
    pub return_code: Option<i32>,
}

pub struct InnerTxDB {
    pub hash: Vec<u8>,
    pub block_id: Vec<u8>,
    pub wrapper_id: Vec<u8>,
    pub code: [u8; 32],
    pub code_type: String,
    pub memo: Vec<u8>,
    pub data: serde_json::Value,
    pub return_code: Option<i32>,
}