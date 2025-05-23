use crate::config::IndexerConfig;
use crate::checksums::Checksums;
use futures::stream::StreamExt;
use futures_util::pin_mut;
use futures_util::Stream;
use std::process;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tendermint::block::Block;
use tendermint::block::Height;
use tendermint::genesis::Genesis;
use tendermint_rpc::endpoint::block_results;
use tendermint_rpc::{self, Client, HttpClient};
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use tracing::{info, instrument, warn};

pub mod utils;
pub mod tx;
pub mod block_result;
pub mod id;

use super::database::Database;
use super::error::Error;

// Time to wait between unsuccesfull calls to http.get_block
const WAIT_FOR_BLOCK: u64 = 10;

// Max number of queued blocks in channel.
// this can be adjusted for optimal performance, however
// either http request or database_queries are both slow
// processes.
const MAX_BLOCKS_IN_CHANNEL: usize = 100;

// Block info required to be saved
type BlockInfo = (Block, block_results::Response);

#[instrument(skip(client))]
async fn get_block(block_height: u32, chain_name: &str, client: &HttpClient) -> BlockInfo {
    loop {
        let height = Height::from(block_height);
        tracing::trace!(message = "Requesting block: ", block_height);

        let instant = tokio::time::Instant::now();

        let response = client.block(block_height).await;

        let dur = instant.elapsed();

        match response {
            Ok(resp) => {
                info!("Got block {}", block_height);

                metrics::histogram!(crate::INDEXER_GET_BLOCK_DURATION, dur.as_secs_f64());

                // update the gauge indicating last block height retrieved.
                metrics::gauge!(
                    crate::INDEXER_LAST_GET_BLOCK_HEIGHT,
                    resp.block.header.height.value() as f64,
                    "chain_name" => chain_name.to_string(),
                );

                // If we successfully retrieved a block we want to get the block result.
                // It is used to know if a transaction has been successfully or not.
                let block_results = get_block_results(height, client).await;

                if let Ok(br) = block_results {
                    return (resp.block, br);
                }
            }

            Err(err) => {
                let labels = [("indexer_get_block_error: ", err.detail().to_string())];
                metrics::histogram!(
                    crate::INDEXER_GET_BLOCK_DURATION,
                    dur.as_secs_f64(),
                    &labels
                );

                match &err.0 {
                    tendermint_rpc::error::ErrorDetail::Response(e) => {
                        tracing::warn!(
                                "Failed to retreive block at height {}. Trying again in 10 seconds. (REASON : {})",
                                block_height,
                                e,
                            );
                        // Wait WAIT_FOR_BLOCK seconds before asking for new block
                        // because it has probably not been validated yet
                        tokio::time::sleep(Duration::from_secs(WAIT_FOR_BLOCK)).await;
                    }
                    tendermint_rpc::error::ErrorDetail::Http(e) => {
                        tracing::warn!(
                            "Failed to retreive block at height {}. (REASON : {})",
                            block_height,
                            e,
                        );
                    }
                    _ => {
                        tracing::warn!(
                            "Failed to retreive block at height {}. (REASON : {})",
                            block_height,
                            err.detail(),
                        );
                    }
                }
            }
        }
    }
}

#[instrument(name = "Indexer::block_results", skip(client))]
async fn get_block_results(
    block_height: Height,
    client: &HttpClient,
) -> Result<block_results::Response, Error> {
    let response = client.block_results(block_height).await;

    match response {
        Ok(r) => Ok(r),
        Err(err) => {
            match &err.0 {
                tendermint_rpc::error::ErrorDetail::Response(e) => {
                    tracing::warn!(
                            "Failed to retreive block at height {}. Trying again in 10 seconds. (REASON : {})",
                            block_height,
                            e,
                        );
                    // Wait WAIT_FOR_BLOCK seconds before asking for new block
                    // because it has probably not been validated yet
                    tokio::time::sleep(Duration::from_secs(WAIT_FOR_BLOCK)).await;
                }
                tendermint_rpc::error::ErrorDetail::Http(e) => {
                    tracing::warn!(
                        "Failed to retreive block at height {}. (REASON : {})",
                        block_height,
                        e,
                    );
                }
                _ => {
                    tracing::warn!(
                        "Failed to retreive block at height {}. (REASON : {})",
                        block_height,
                        err.detail(),
                    );
                }
            }

            Err(Error::TendermintRpcError(err))
        }
    }
}

#[allow(clippy::let_with_type_underscore)]
#[instrument(name = "Indexer::blocks_stream", skip(client, block))]
fn blocks_stream<'a>(
    block: u64,
    chain_name: &'a str,
    client: &'a HttpClient,
) -> impl Stream<Item = BlockInfo> + 'a {
    futures::stream::iter(block..)
        .then(move |i| async move { get_block(i as u32, chain_name, client).await })
}

/// Start the indexer service blocking current thread.
/// # Arguments:
///
/// `db` The (database)[Database] to use for storing data.
///
/// `config` The configuration containing required information used to connect to namada node
/// to retrieve blocks from.
pub async fn start_indexing(
    client: HttpClient,
    db: Database,
    chain_name: &str,
    create_index: bool,
    checksums: &Checksums,
) -> Result<(), Error> {
    info!("***** Starting indexer *****");

    /********************
     *
     *  Verify if we resume indexing
     *
     ********************/

    let mut current_height = utils::get_start_height(&db).await?;
    info!("Starting at height : {}", &current_height);

    // check if indexes has been created in the database
    let has_indexes = utils::has_indexes(&db).await?;

    /********************
     *
     *  Init RPC
     *
     ********************/
    info!("Getting last block");
    let latest_block = client.latest_block().await?;
    info!("Current block tip {}", &latest_block.block.header.height);

    // Do we have the rpc for the block height we want ? For that we check the genesis block
    let genesis_block: Genesis<Option<serde_json::Value>> = client.genesis().await?;

    if genesis_block.initial_height > current_height.into() {
        warn!("RPC doesn't allow fetching previous block. Please restart with a pre-fork RPC to sync pre-fork blocks.");
        // we must exit here because we can't use the same client for pre-fork and post-fork
        process::exit(0);
    }

    /********************
     *
     *  Start indexing
     *
     ********************/

    let shutdown = Arc::new(AtomicBool::new(false));

    let producer_shutdown = shutdown.clone();

    // Spaw block producer task, this could speed up saving blocks
    // because it does not need to wait for database to finish saving a block.
    let (mut rx, producer_handler) =
        spawn_block_producer(current_height as _, chain_name, client, producer_shutdown);

    // Block consumer that stores block into the database
    while let Some(block) = rx.recv().await {
        // block is now the block info and the block results
        if let Err(e) = db.save_block(&block.0, &block.1, &checksums).await {
            // shutdown producer task
            shutdown.store(true, Ordering::Relaxed);
            tracing::error!("Closing block producer task due to an error saving last block: {e}");

            // propagate the error
            return Err(e);
        }

        info!("Block: {} saved", block.0.header.height.value());

        let height = Height::from(current_height);

        if (current_height + 1) as i64 == genesis_block.initial_height {
            warn!("We have synced pre-fork data. The indexer will stop. Please restart it with the post-fork RPC url.");
            process::exit(0);
        }

        // create indexes if they have not been created yet
        if !has_indexes && latest_block.block.header.height == height {
            info!("We are synced!");

            if create_index {
                info!("Creating indexes");
                db.create_indexes().await?;

                info!("Indexing done");
            }
        }

        current_height += 1;
    }

    // propagate any error from the block producer
    // like failing to connect to namada node for any reason
    // and so on.
    producer_handler.await??;

    Ok(())
}

fn spawn_block_producer(
    current_height: u64,
    chain_name: &str,
    client: HttpClient,
    producer_shutdown: Arc<AtomicBool>,
) -> (Receiver<BlockInfo>, JoinHandle<Result<(), Error>>) {
    // Create a channel
    let (tx, rx): (Sender<BlockInfo>, Receiver<BlockInfo>) =
        tokio::sync::mpsc::channel(MAX_BLOCKS_IN_CHANNEL);

    // Spawn the task
    let chain_name = chain_name.to_string();
    let handler = tokio::spawn(async move {
        let stream = blocks_stream(current_height as _, chain_name.as_str(), &client);
        pin_mut!(stream);

        while let Some(block) = stream.next().await {
            if producer_shutdown.load(Ordering::Relaxed) {
                tracing::warn!("Block consumer closed, exiting producer");
                break;
            }

            tx.send(block).await?;
        }

        Ok::<(), Error>(())
    });

    (rx, handler)
}
