mod config;
pub mod database;
mod error;
mod indexer;
pub(crate) mod queries;
pub mod server;
pub mod tables;
mod telemetry;
pub mod utils;
mod views;
pub mod checksums;

pub use crate::config::{
    IndexerConfig, JaegerConfig, LogFormat, PrometheusConfig, ServerConfig, Settings,
};
pub use database::Database;
pub use error::Error;
pub use indexer::start_indexing;
pub use server::{create_server, start_server, BlockInfo};
pub use telemetry::{get_subscriber, init_subscriber, setup_logging};

pub const INDEXER_GET_BLOCK_DURATION: &str = "indexer_get_block_duration";
const DB_SAVE_BLOCK_COUNTER: &str = "db_save_count_block";
const DB_SAVE_BLOCK_DURATION: &str = "db_save_duration_block";
const DB_SAVE_TXS_DURATION: &str = "db_save_duration_transactions";
const DB_SAVE_TXS_BATCH_SIZE: &str = "db_save_batch_size_transactions";
const DB_SAVE_EVDS_DURATION: &str = "db_save_duration_evidences";
const DB_SAVE_EVDS_BATCH_SIZE: &str = "db_save_batch_size_evidences";
const DB_SAVE_COMMIT_SIG_DURATION: &str = "db_save_duration_commit_sig";
const DB_SAVE_COMMIT_SIG_BATCH_SIZE: &str = "db_save_batch_size_commit_sig";
const INDEXER_LAST_SAVE_BLOCK_HEIGHT: &str = "indexer_last_save_block_height";
const INDEXER_LAST_GET_BLOCK_HEIGHT: &str = "indexer_last_get_block_height";

pub const MASP_ADDR: &str = "tnam1pcqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqzmefah";
