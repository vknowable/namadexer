use crate::error::Error;
use crate::CHECKSUMS;
use namada_core::masp::MaspTxId;
use namada_sdk::tx::data::TxType;
use std::collections::{HashMap, BTreeMap};
use std::{env, fs};
use namada_sdk::token::{Transfer, DenominatedAmount};
use serde::Serialize;

const CHECKSUMS_FILE_PATH_ENV: &str = "CHECKSUMS_FILE_PATH";
const CHECKSUMS_REMOTE_URL_ENV: &str = "CHECKSUMS_REMOTE_URL";
const CHECKSUMS_DEFAULT_PATH: &str = "checksums.json";

pub fn tx_type_name(tx_type: &TxType) -> String {
    match tx_type {
        TxType::Raw => "Raw".to_string(),
        TxType::Wrapper(_) => "Wrapper".to_string(),
        // TxType::Decrypted(_) => "Decrypted".to_string(),
        TxType::Protocol(_) => "Protocol".to_string(),
    }
}

pub fn load_checksums() -> Result<HashMap<String, String>, crate::Error> {
    let checksums_file_path = env::var(CHECKSUMS_FILE_PATH_ENV);
    let checksums_remote_url = env::var(CHECKSUMS_REMOTE_URL_ENV);

    let checksums = match (checksums_file_path, checksums_remote_url) {
        (Ok(path), _) => fs::read_to_string(path)?,
        (_, Ok(url)) => ureq::get(&url)
            .call()
            .map_err(|e| crate::Error::Generic(Box::new(e)))?
            .into_string()?,
        _ => fs::read_to_string(CHECKSUMS_DEFAULT_PATH)?,
    };

    let json: serde_json::Value = serde_json::from_str(&checksums)?;
    let obj = json.as_object().ok_or(crate::Error::InvalidChecksum)?;

    let mut checksums_map = HashMap::new();
    for value in obj.iter() {
        let hash = value
            .1
            .as_str()
            .ok_or(crate::Error::InvalidChecksum)?
            .split('.')
            .collect::<Vec<&str>>()[1];
        let type_tx = value.0.split('.').collect::<Vec<&str>>()[0];

        checksums_map.insert(hash.to_string(), type_tx.to_string());
    }

    Ok(checksums_map)
}

// Function to create a reversed map from the existing CHECKSUMS
pub fn reverse_checksums() -> HashMap<String, String> {
    let mut reverse_map = HashMap::new();
    for (hash, tx_type) in CHECKSUMS.iter() {
        reverse_map.insert(tx_type.clone(), hash.clone());
    }
    reverse_map
}

// Wrapper struct for serializing Transfer
#[derive(Serialize)]
pub struct TransferJson {
    sources: BTreeMap<String, DenominatedAmount>,
    targets: BTreeMap<String, DenominatedAmount>,
    shielded_section_hash: Option<MaspTxId>, // If `MaspTxId` can also be converted to String
}

impl TransferJson {
    pub fn from_transfer(transfer: Transfer) -> Result<Self, Error> {

        let sources = transfer
            .sources
            .into_iter()
            .map(|(key, value)| Ok((key.owner.to_string(), value)))
            .collect::<Result<BTreeMap<String, DenominatedAmount>, Error>>()?;
        
        let targets = transfer
            .targets
            .into_iter()
            .map(|(key, value)| Ok((key.owner.to_string(), value)))
            .collect::<Result<BTreeMap<String, DenominatedAmount>, Error>>()?;
        
        Ok(TransferJson {
            sources,
            targets,
            shielded_section_hash: transfer.shielded_section_hash,
        })
    }
}