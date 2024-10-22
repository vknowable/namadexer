use axum::{
    extract::{Path, State},
    Json,
};
use tracing::info;

use crate::{
    server::{ServerState, tx::TransferTxInfo},
    Error,
};

pub async fn get_txs_by_address(
    State(state): State<ServerState>,
    Path(address): Path<String>,
) -> Result<Json<Option<Vec<TransferTxInfo>>>, Error> {
    info!("calling /address/:{}", address);

    let rows = state.db.get_txs_by_address(&address).await?;

    if rows.is_empty() {
        return Ok(Json(None));
    }

    let mut response: Vec<TransferTxInfo> = vec![];
    for row in rows {
        let tx = TransferTxInfo::try_from(row)?;
        response.push(tx);
    }

    Ok(Json(Some(response)))
}
