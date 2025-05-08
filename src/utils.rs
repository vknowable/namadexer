use namada_sdk::{chain::BlockHeight, hash::Hash, queries::RPC, storage::Key, tx::data::TxType};
use tendermint_rpc::HttpClient;
use crate::checksums::Checksums;

pub fn tx_type_name(tx_type: &TxType) -> String {
    match tx_type {
        TxType::Raw => "Raw".to_string(),
        TxType::Wrapper(_) => "Wrapper".to_string(),
        TxType::Protocol(_) => "Protocol".to_string(),
    }
}

pub async fn fetch_checksums_from_node(client: &HttpClient) -> Result<Checksums, crate::Error> {
    let mut checksums = Checksums::default();
    for code_path in Checksums::code_paths() {
        let code = query_tx_code_hash(&client, &code_path)
            .await
            .unwrap_or_else(|| {
                panic!("{} must be defined in namada storage.", code_path)
            });
        checksums.add(code_path, code.to_lowercase());
    }
    Ok(checksums)
}

async fn query_tx_code_hash(
    client: &HttpClient,
    tx_code_path: &str,
) -> Option<String> {
    let storage_key = Key::wasm_hash(tx_code_path);
    let tx_code_res =
        query_storage_bytes(client, &storage_key, None).await.ok()?;
    if let Some(tx_code_bytes) = tx_code_res {
        let tx_code =
            Hash::try_from(&tx_code_bytes[..]).expect("Invalid code hash");
        Some(tx_code.to_string())
    } else {
        None
    }
}

pub async fn query_storage_bytes(
    client: &HttpClient,
    key: &Key,
    height: Option<BlockHeight>,
) -> Result<Option<Vec<u8>>, crate::Error> {
    match RPC.shell().storage_value(client, None, height, false, key).await {
        Ok(value) => {
            if value.data.is_empty() {
                Ok(None)
            } else {
                Ok(Some(value.data))
            }
        }
        Err(e) => {
            Err(crate::Error::Generic(Box::new(e)))
        }
    }
}
