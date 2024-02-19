use axum::{
  extract::{Path, Query, State},
  Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row as TRow;
use std::collections::HashMap;
use tracing::info;

use crate::{
  server::{
      ServerState,
      signatures::{Signature, SignaturesInfo},
  },
  Error,
};

pub async fn get_signatures_by_block_hash(
  State(state): State<ServerState>,
  Path(block_hash): Path<String>,
) -> Result<Json<Option<SignaturesInfo>>, Error> {
  info!("calling /block/signatures/:block_hash");

  let id = hex::decode(block_hash.clone())?;
  let rows = state.db.signatures_by_block_hash(&id).await?;
  
  let mut all_signatures = Vec::new();
  for row in rows {
    let signature = Signature::try_from(&row)?;
    all_signatures.push(signature);
  }

  let signatures = SignaturesInfo {
    block_id: block_hash,
    signatures: all_signatures,
  };

  Ok(Json(Some(signatures)))
}