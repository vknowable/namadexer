use crate::checksums::Checksums;
use crate::queries::insert_block_query;
use crate::{config::DatabaseConfig, error::Error};
use strum::IntoEnumIterator;

use namada_sdk::tx::{data::TxType, Tx};
use namada_tx::{data::compute_inner_tx_hash, either::Either};
use sqlx::postgres::{PgPool, PgPoolOptions, PgRow as Row};
use sqlx::{query, QueryBuilder, Transaction};
use std::sync::Arc;
use std::time::Duration;
use tendermint::block::Block;
use tendermint_proto::types::evidence::Sum;
use tendermint_proto::types::CommitSig;
use tendermint_proto::types::EvidenceList as RawEvidenceList;
use tendermint_rpc::endpoint::block_results;
use tracing::{debug, info, instrument};

use crate::{
    DB_SAVE_BLOCK_COUNTER, DB_SAVE_BLOCK_DURATION, DB_SAVE_COMMIT_SIG_BATCH_SIZE,
    DB_SAVE_COMMIT_SIG_DURATION, DB_SAVE_EVDS_BATCH_SIZE, DB_SAVE_EVDS_DURATION,
    DB_SAVE_TXS_BATCH_SIZE, DB_SAVE_TXS_DURATION, INDEXER_LAST_SAVE_BLOCK_HEIGHT, MASP_ADDR,
};

use crate::tables::{
    get_create_block_table_query, get_create_commit_signatures_table_query,
    get_create_evidences_table_query,
    get_create_inner_transactions_table_query, get_create_wrapper_transactions_table_query,
};
use crate::views::{self, ViewType};
use crate::indexer::tx::{decode_tx, insert_inner_txs, insert_wrapper_txs, WrapperTxDB, InnerTxDB};
use crate::indexer::block_result::{BlockResult, TransactionExitStatus};
use crate::indexer::id::Id;

use metrics::{gauge, histogram, increment_counter};

const BLOCKS_TABLE_NAME: &str = "blocks";
const WRAPPER_TX_TABLE_NAME: &str = "wrapper_transactions";
const INNER_TX_TABLE_NAME: &str = "inner_transactions";

// Max time to wait for a succesfull database connection
const DATABASE_TIMEOUT: u64 = 60;

#[derive(Clone)]
pub struct Database {
    pool: Arc<PgPool>,
    // we use the network as the name of the schema to allow diffrent net on the same database
    pub network: String,
}

impl Database {
    pub async fn new(db_config: &DatabaseConfig, network: &str) -> Result<Database, Error> {
        // sqlx expects config of the form:
        // postgres://user:password@host:port/db_name
        let config = format!(
            // In order to accommodate the use of a unix socket, we are actually supplying a dummy
            // string "not-the-host" to the host portion of the URL, and putting the actual host in the
            // query string. This is the only way to provide a "host" to sqlx that contains slashes.
            // When "host=" is given as a query parameter, postgres ignores the host portion of the URL
            // though it is still required to be present for the URL to parse correctly.
            "postgres://{}:{}@not-the-host:{}/{}?host={}",
            db_config.user, db_config.password, db_config.port, db_config.dbname, db_config.host
        );

        // If timeout setting is not present in the provided configuration,
        // lets use our default timeout.
        let timeout = db_config.connection_timeout.unwrap_or(DATABASE_TIMEOUT);

        debug!(
            "connecting to database at {} with timeout {}",
            config.replace(&db_config.password, "*****"),
            timeout
        );

        let pool = PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(timeout))
            .connect(&config)
            .await?;

        let network_schema = network.replace('-', "_");

        Ok(Database {
            pool: Arc::new(pool),
            network: network_schema,
        })
    }

    pub fn with_pool(pool: PgPool, network: String) -> Self {
        Self {
            pool: Arc::new(pool),
            network,
        }
    }

    pub async fn transaction<'a>(&self) -> Result<sqlx::Transaction<'a, sqlx::Postgres>, Error> {
        self.pool.begin().await.map_err(Error::from)
    }

    /// Create required tables in the database.
    /// these tables are:
    /// - `blocks` to store relevant information about blocks, for example its id, commits
    /// and block_header
    /// - `transactions` although part of the block data, they are store in a different table
    /// and contain useful information about transactions.
    /// - `evidences` Where block's evidence data is stored.
    #[instrument(skip(self))]
    pub async fn create_tables(&self, checksums: &Checksums) -> Result<(), Error> {
        info!("Creating tables if they don't exist");

        query(&format!("CREATE SCHEMA IF NOT EXISTS {}", self.network))
            .execute(&*self.pool)
            .await?;

        query(get_create_block_table_query(&self.network).as_str())
            .execute(&*self.pool)
            .await?;

        query(get_create_commit_signatures_table_query(&self.network).as_str())
            .execute(&*self.pool)
            .await?;

        query(get_create_inner_transactions_table_query(&self.network).as_str())
            .execute(&*self.pool)
            .await?;

        query(get_create_wrapper_transactions_table_query(&self.network).as_str())
            .execute(&*self.pool)
            .await?;

        query(get_create_evidences_table_query(&self.network).as_str())
            .execute(&*self.pool)
            .await?;

        // Drop any existing views
        for view in ViewType::iter() {
            let drop_query = views::get_drop_view_query(&self.network, &view);
            query(drop_query.as_str())
                .execute(&*self.pool)
                .await?;
        }

        // Create transaction views
        for view in ViewType::iter() {
            let create_query = views::get_create_view_query(&self.network, &view, checksums);
            query(create_query.as_str())
                .execute(&*self.pool)
                .await?;
        }

        Ok(())
    }

    /// Inner implementation that uses a postgres-transaction
    /// to ensure database coherence.
    #[instrument(skip(block, block_results, sqlx_tx))]
    async fn save_block_impl<'a>(
        block: &Block,
        block_results: &block_results::Response,
        sqlx_tx: &mut Transaction<'a, sqlx::Postgres>,
        network: &str,
        checksums: &Checksums,
    ) -> Result<(), Error> {
        // let mut query_builder: QueryBuilder<_> = QueryBuilder::new(insert_block_query(network));

        let block_id = block.header.hash();
        let block_id = block_id.as_bytes();

        // use persistent query for database to optimize it.
        let query_str = insert_block_query(network);
        let query = sqlx::query(&query_str).persistent(true);

        let query = query
            .bind(block_id)
            .bind(block.header.version.app as i32)
            .bind(block.header.version.block as i32)
            .bind(block.header.chain_id.as_str())
            .bind(block.header.height.value() as i32)
            .bind(block.header.time.to_rfc3339())
            .bind(
                block
                    .header
                    .last_block_id
                    .map(|id| id.hash.as_bytes().to_vec()),
            )
            .bind(
                block
                    .header
                    .last_block_id
                    .map(|id| id.part_set_header.total as i32),
            )
            .bind(
                block
                    .header
                    .last_block_id
                    .map(|id| id.part_set_header.hash.as_bytes().to_vec()),
            )
            .bind(
                block
                    .header
                    .last_commit_hash
                    .map(|lch| lch.as_bytes().to_vec()),
            )
            .bind(block.header.data_hash.map(|dh| dh.as_bytes().to_vec()))
            .bind(block.header.validators_hash.as_bytes().to_vec())
            .bind(block.header.next_validators_hash.as_bytes().to_vec())
            .bind(block.header.consensus_hash.as_bytes().to_vec())
            .bind(block.header.app_hash.to_string())
            .bind(
                block
                    .header
                    .last_results_hash
                    .map(|lrh| lrh.as_bytes().to_vec()),
            )
            .bind(block.header.evidence_hash.map(|eh| eh.as_bytes().to_vec()))
            .bind(block.header.proposer_address.to_string())
            .bind(block.last_commit.as_ref().map(|c| c.height.value() as i32))
            .bind(block.last_commit.as_ref().map(|c| c.round.value() as i32))
            .bind(
                block
                    .last_commit
                    .as_ref()
                    .map(|c| c.block_id.hash.as_bytes().to_vec()),
            )
            .bind(
                block
                    .last_commit
                    .as_ref()
                    .map(|c| c.block_id.part_set_header.total as i32),
            )
            .bind(
                block
                    .last_commit
                    .as_ref()
                    .map(|c| c.block_id.part_set_header.hash.as_bytes().to_vec()),
            );

        query.execute(&mut *sqlx_tx).await?;

        let commit_signatures = block.last_commit.as_ref().map(|c| &c.signatures);

        // Check if we have commit signatures
        if let Some(cs) = commit_signatures {
            let signatures: Vec<CommitSig> =
                cs.iter().map(|s| CommitSig::from(s.to_owned())).collect();
            Self::save_commit_sinatures(block_id, &signatures, sqlx_tx, network).await?;
        };

        let evidence_list = RawEvidenceList::from(block.evidence().clone());
        Self::save_evidences(evidence_list, block_id, sqlx_tx, network).await?;
        Self::save_transactions(
            block.data.as_ref(),
            block_id,
            block.header.height.value(),
            block_results,
            sqlx_tx,
            network,
            checksums,
        )
        .await?;

        Ok(())
    }

    /// Save a block and commit database
    #[instrument(skip(self, block, block_results))]
    pub async fn save_block(
        &self,
        block: &Block,
        block_results: &block_results::Response,
        checksums: &Checksums,
    ) -> Result<(), Error> {
        let instant = tokio::time::Instant::now();
        // Lets use postgres transaction internally for 2 reasons:
        // - A block could contain many evidences and Txs, so this approach allows
        // saving all of them and commit at the end.
        // - Errors could happen in the middle either while processing
        // transactions, evidences or blocks. with postgres-transaction
        // we ensure database integrity, and commit only if all operations
        // succeeded.
        let mut sqlx_tx = self.transaction().await?;

        Self::save_block_impl(block, block_results, &mut sqlx_tx, self.network.as_str(), checksums).await?;

        let res = sqlx_tx.commit().await.map_err(Error::from);

        let dur = instant.elapsed();

        let mut status = "Ok".to_string();
        if let Err(e) = &res {
            status = e.to_string();
        }

        let labels = [("status", status)];

        histogram!(DB_SAVE_BLOCK_DURATION, dur.as_secs_f64() * 1000.0, &labels);

        if res.is_ok() {
            // update our counter for processed blocks since service started.
            increment_counter!(DB_SAVE_BLOCK_COUNTER, &labels);

            // update the gauge indicating last block height saved.
            gauge!(INDEXER_LAST_SAVE_BLOCK_HEIGHT,
                block.header.height.value() as f64,
                "chain_name" => self.network.clone());
        }

        res
    }

    /// Save a block and commit database
    #[instrument(skip(block_id, signatures, sqlx_tx, network))]
    pub async fn save_commit_sinatures<'a>(
        block_id: &[u8],
        signatures: &Vec<CommitSig>,
        sqlx_tx: &mut Transaction<'a, sqlx::Postgres>,
        network: &str,
    ) -> Result<(), Error> {
        debug!("saving commit signatures");

        let mut query_builder: QueryBuilder<_> = QueryBuilder::new(format!(
            "INSERT INTO {}.commit_signatures(
                block_id,
                block_id_flag,
                validator_address,
                timestamp,
                signature
            )",
            network
        ));

        let instant = tokio::time::Instant::now();

        // Preparing data before inserting it
        // in the commit_signatures table.
        let mut signature_data = Vec::new();

        for signature in signatures {
            signature_data.push((
                block_id,
                signature.block_id_flag,
                signature.validator_address.clone(),
                signature.timestamp.as_ref().map(|t| t.seconds.to_string()),
                signature.signature.clone(),
            ));
        }

        let num_signatures = signature_data.len();

        if num_signatures == 0 {
            let labels = [
                ("bulk_insert", "signatures".to_string()),
                ("status", "Ok".to_string()),
            ];
            let dur = instant.elapsed();
            histogram!(
                DB_SAVE_COMMIT_SIG_DURATION,
                dur.as_secs_f64() * 1000.0,
                &labels
            );
            histogram!(
                DB_SAVE_COMMIT_SIG_BATCH_SIZE,
                num_signatures as f64,
                &labels
            );

            return Ok(());
        }

        let res = query_builder
            .push_values(
                signature_data.into_iter(),
                |mut b, (block_id, block_id_flag, validator_address, timestamp, signature)| {
                    b.push_bind(block_id)
                        .push_bind(block_id_flag)
                        .push_bind(validator_address)
                        .push_bind(timestamp)
                        .push_bind(signature);
                },
            )
            .build()
            .execute(&mut *sqlx_tx)
            .await
            .map(|_| ())
            .map_err(Error::from);

        let dur = instant.elapsed();

        let mut status = "Ok".to_string();
        if let Err(e) = &res {
            status = e.to_string();
        }

        let labels = [
            ("bulk_insert", "signatures".to_string()),
            ("status", status),
        ];

        histogram!(
            DB_SAVE_COMMIT_SIG_DURATION,
            dur.as_secs_f64() * 1000.0,
            &labels
        );
        histogram!(
            DB_SAVE_COMMIT_SIG_BATCH_SIZE,
            num_signatures as f64,
            &labels
        );

        res
    }

    /// Save a block, the operation uses a sqlx::Transaction,
    /// It is up to the caller to commit the operation.
    /// this method is meant to be used when caller is saving
    /// many blocks, and can commit after it.
    #[instrument(skip(block, block_results, sqlx_tx, network))]
    pub async fn save_block_tx<'a>(
        block: &Block,
        block_results: &block_results::Response,
        sqlx_tx: &mut Transaction<'a, sqlx::Postgres>,
        network: &str,
        checksums: &Checksums,
    ) -> Result<(), Error> {
        Self::save_block_impl(block, block_results, sqlx_tx, network, checksums).await
    }

    /// Save all the evidences in the list, it is up to the caller to
    /// call sqlx_tx.commit().await?; for the changes to take place in
    /// database.
    #[instrument(skip(evidences, block_id, sqlx_tx, network))]
    async fn save_evidences<'a>(
        evidences: RawEvidenceList,
        block_id: &[u8],
        sqlx_tx: &mut Transaction<'a, sqlx::Postgres>,
        network: &str,
    ) -> Result<(), Error> {
        debug!("saving evidences");

        let mut query_builder: QueryBuilder<_> = QueryBuilder::new(format!(
            "INSERT INTO {}.evidences(
                    block_id,
                    height,
                    time,
                    address,
                    total_voting_power,
                    validator_power
            )",
            network
        ));

        let instant = tokio::time::Instant::now();

        // Same as transactions regarding limitations in field binding
        // in postgres, but it is unlikely to have such hight amount
        // of evidences per block.
        // (block_id, height, time, address, total_voting_power, validator_power)
        let mut evidences_data = Vec::new();

        for evidence in evidences.evidence {
            let Some(s) = evidence.sum else {
                tracing::debug!("No evidence");
                continue;
            };

            match s {
                Sum::DuplicateVoteEvidence(dve) => {
                    evidences_data.push((
                        block_id,
                        dve.vote_a.as_ref().map(|v| v.height),
                        dve.vote_a
                            .as_ref()
                            .and_then(|v| v.timestamp.as_ref())
                            .map(|t| t.seconds.to_string()),
                        dve.vote_a.as_ref().map(|v| v.validator_address.clone()),
                        dve.total_voting_power,
                        dve.validator_power,
                    ));
                }
                _ => tracing::warn!("Unknown evidence."),
            }
        }

        let num_evidences = evidences_data.len();

        if num_evidences == 0 {
            let labels = [
                ("bulk_insert", "evidences".to_string()),
                ("status", "Ok".to_string()),
            ];
            let dur = instant.elapsed();

            histogram!(DB_SAVE_EVDS_DURATION, dur.as_secs_f64() * 1000.0, &labels);
            histogram!(DB_SAVE_EVDS_BATCH_SIZE, num_evidences as f64, &labels);

            return Ok(());
        }

        let res = query_builder
            .push_values(
                evidences_data.into_iter(),
                |mut b, (block_id, height, time, address, total_voting_power, validator_power)| {
                    b.push_bind(block_id)
                        .push_bind(height)
                        .push_bind(time)
                        .push_bind(address)
                        .push_bind(total_voting_power)
                        .push_bind(validator_power);
                },
            )
            .build()
            .execute(&mut *sqlx_tx)
            .await
            .map(|_| ())
            .map_err(Error::from);

        let dur = instant.elapsed();

        let mut status = "Ok".to_string();
        if let Err(e) = &res {
            status = e.to_string();
        }

        let labels = [("bulk_insert", "evidences".to_string()), ("status", status)];

        histogram!(DB_SAVE_EVDS_DURATION, dur.as_secs_f64() * 1000.0, &labels);
        histogram!(DB_SAVE_EVDS_BATCH_SIZE, num_evidences as f64, &labels);

        res
    }

    /// Save all the transactions in txs, it is up to the caller to
    /// call sqlx_tx.commit().await?; for the changes to take place in
    /// database.
    #[instrument(skip(wrapper_txs, block_id, sqlx_tx, raw_block_results, network))]
    async fn save_transactions<'a>(
        wrapper_txs: &[Vec<u8>],
        block_id: &[u8],
        block_height: u64,
        raw_block_results: &block_results::Response,
        sqlx_tx: &mut Transaction<'a, sqlx::Postgres>,
        network: &str,
        checksums: &Checksums,
    ) -> Result<(), Error> {
        // use for metrics
        let instant = tokio::time::Instant::now();

        if wrapper_txs.is_empty() {
            let labels = [
                ("bulk_insert", "transactions".to_string()),
                ("status", "Ok".to_string()),
            ];

            let dur = instant.elapsed();

            histogram!(DB_SAVE_TXS_DURATION, dur.as_secs_f64() * 1000.0, &labels);
            histogram!(DB_SAVE_TXS_BATCH_SIZE, 0.0, &labels);

            return Ok(());
        }

        debug!(message = "Saving transactions");

        // this will holds tx tuples
        // in order to push wrapper_txs.len at once in a single query.
        // the limit for bind values in postgres is 65535 values, that means that
        // to hit that limit a block would need to have:
        // n_tx = 65535/8 = 8191
        // being 8 the number of columns.
        let mut wrapper_tx_values = Vec::<WrapperTxDB>::with_capacity(wrapper_txs.len());
        let mut inner_tx_values = Vec::<InnerTxDB>::new();

        let block_result = BlockResult::from(raw_block_results);

        // for each wrapper tx ...
        for t in wrapper_txs.iter() {
            let wrapper_tx = Tx::try_from(t.as_slice()).map_err(|e| Error::InvalidTxData(e.to_string()))?;
            let wrapper_id = wrapper_tx.header_hash().to_vec();
            let wrapper_id_str = Id::Hash(hex::encode(&wrapper_id).to_lowercase());
            let wrapper_return_code = match block_result.is_wrapper_tx_applied(&wrapper_id_str) {
                TransactionExitStatus::Applied => Some(0),
                TransactionExitStatus::Rejected => Some(1),
            };

            // ... iterate over all inner txs
            let commitments = wrapper_tx.commitments().to_owned();
            for inner_tx in commitments.iter() {

                let memo: Vec<u8> = wrapper_tx.memo(&inner_tx).unwrap_or_default();
                let code = wrapper_tx
                    .get_section(&inner_tx.code_hash)
                    .and_then(|s| s.code_sec())
                    .map(|s| s.code.hash().0)
                    .ok_or(Error::InvalidTxData("no code hash".into()))?;

                let code_hex = hex::encode(code.as_slice());
                let unknown_type = "unknown".to_string();
                let type_tx = checksums.get_name_by_id(&code_hex).unwrap_or(unknown_type);
                println!("type_tx: {:?}", type_tx);

                let inner_hash = compute_inner_tx_hash(Some(&wrapper_tx.header_hash()), Either::Right(&inner_tx)).to_vec();
                let inner_hash_str = Id::Hash(hex::encode(&inner_hash).to_lowercase());
                let inner_return_code = match block_result.is_inner_tx_accepted(&wrapper_id_str, &inner_hash_str) {
                    TransactionExitStatus::Applied => Some(0),
                    TransactionExitStatus::Rejected => Some(1),
                };

                let data = wrapper_tx
                    .data(&inner_tx)
                    .ok_or(Error::InvalidTxData("tx has no data".into()))?;

                let code_type = type_tx.to_string();

                info!("Saving {} transaction", type_tx);

                let data_json = decode_tx(&type_tx, &data)?;

                inner_tx_values.push(InnerTxDB {
                    hash: inner_hash, // PLACEHOLDER
                    block_id: block_id.to_vec(),
                    wrapper_id: wrapper_id.clone(),
                    code,
                    code_type,
                    memo,
                    data: data_json,
                    return_code: inner_return_code,
                });
            }

            // index the remaining wrapper tx info
            let atomic = wrapper_tx.header().atomic;
            let mut fee_payer: Option<String> = None;
            let mut fee_amount_per_gas_unit: Option<String> = None;
            let mut fee_token: Option<String> = None;
            let mut fee_gas_limit_multiplier: Option<i64> = None;
            if let TxType::Wrapper(txw) = wrapper_tx.header().tx_type {
                fee_amount_per_gas_unit = Some(txw.fee.amount_per_gas_unit.to_string_precise());
                fee_token = Some(txw.fee.token.to_string());
                let multiplier: u64 = txw.gas_limit.into();
                // WARNING! converting into i64 might ended up changing the value but there is little
                // chance that he goes higher than i64 max value
                fee_gas_limit_multiplier = Some(multiplier as i64);
                fee_payer = Some(txw.fee_payer().to_string());
            }

            wrapper_tx_values.push(WrapperTxDB {
                hash: wrapper_id,
                block_id: block_id.to_vec(),
                fee_payer,
                fee_amount_per_gas_unit,
                fee_token,
                fee_gas_limit_multiplier,
                atomic,
                return_code: wrapper_return_code,
            });
        }

        let res_wrapper = insert_wrapper_txs(&wrapper_tx_values, sqlx_tx, network).await;
        let res_inner = insert_inner_txs(&inner_tx_values, sqlx_tx, network).await;
        let dur = instant.elapsed();

        let mut status = "Ok".to_string();
        if res_wrapper.is_err() || res_inner.is_err() {
            if let Err(e) = &res_wrapper {
                status = e.to_string();
            } else if let Err(e) = &res_inner {
                status = e.to_string();
            }
        }

        let labels = [
            ("bulk_insert", "transactions".to_string()),
            ("status", status),
        ];

        let num_inner = inner_tx_values.len();
        histogram!(DB_SAVE_TXS_DURATION, dur.as_secs_f64() * 1000.0, &labels);
        histogram!(DB_SAVE_TXS_BATCH_SIZE, num_inner as f64, &labels);

        // Return the first encountered error, if any
        if res_wrapper.is_err() {
            return res_wrapper;
        }
        if res_inner.is_err() {
            return res_inner;
        }
        res_wrapper
    }

    pub async fn create_indexes(&self) -> Result<(), Error> {
        // we create indexes on the tables to facilitate querying data
        query(
            format!(
                "
                ALTER TABLE {}.blocks ADD CONSTRAINT pk_block_id PRIMARY KEY (block_id);
            ",
                self.network
            )
            .as_str(),
        )
        .execute(&*self.pool)
        .await?;

        query(
            format!(
                "
                ALTER TABLE {}.wrapper_transactions ADD CONSTRAINT pk_wrapper_hash PRIMARY KEY (hash);
            ",
                self.network
            )
            .as_str(),
        )
        .execute(&*self.pool)
        .await?;

        query(
            format!(
                "
                ALTER TABLE {}.inner_transactions ADD CONSTRAINT pk_inner_hash PRIMARY KEY (hash);
            ",
                self.network
            )
            .as_str(),
        )
        .execute(&*self.pool)
        .await?;

        query(
            format!(
                "CREATE UNIQUE INDEX ux_header_height ON {}.blocks (header_height);",
                self.network
            )
            .as_str(),
        )
        .execute(&*self.pool)
        .await?;

        // If a failed transaction is resent and successfull we don't have a unique private key in the tx hash...
        // query(
        //     format!(
        //         "ALTER TABLE {}.transactions ADD CONSTRAINT pk_hash PRIMARY KEY (hash);",
        //         self.network
        //     )
        //     .as_str(),
        // )
        // .execute(&*self.pool)
        // .await?;

        query(format!("ALTER TABLE {0}.wrapper_transactions ADD CONSTRAINT fk_block_id FOREIGN KEY (block_id) REFERENCES {0}.blocks (block_id);", self.network).as_str())
            .execute(&*self.pool)
            .await?;

        query(format!("ALTER TABLE {0}.inner_transactions ADD CONSTRAINT fk_wrapper_id FOREIGN KEY (wrapper_id) REFERENCES {0}.wrapper_transactions (hash);", self.network).as_str())
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    #[instrument(skip(self, block_id))]
    pub async fn block_by_id(&self, block_id: &[u8]) -> Result<Option<Row>, Error> {
        let str = format!("SELECT b.*, inner_txs, wrapper_txs FROM {0}.{BLOCKS_TABLE_NAME} b 
            LEFT JOIN (SELECT block_id, JSON_AGG(JSON_BUILD_OBJECT('hash_id', encode(it.hash, 'hex'), 'wrapper_id', encode(it.wrapper_id, 'hex'), 'code_type', it.code_type)) 
            AS inner_txs FROM {0}.{INNER_TX_TABLE_NAME} it GROUP BY it.block_id) it ON b.block_id = it.block_id 
            LEFT JOIN (SELECT block_id, JSON_AGG(JSON_BUILD_OBJECT('hash_id', encode(wt.hash, 'hex'))) 
            AS wrapper_txs FROM {0}.{WRAPPER_TX_TABLE_NAME} wt GROUP BY wt.block_id) wt ON b.block_id = wt.block_id 
            WHERE b.block_id = $1;", self.network);
        query(&str)
            .bind(block_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(Error::from)
    }

    /// Returns the block at `block_height` if present, otherwise returns an Error.
    #[instrument(skip(self))]
    pub async fn block_by_height(&self, block_height: u32) -> Result<Option<Row>, Error> {
        let str = format!("SELECT b.*, inner_txs, wrapper_txs FROM {0}.{BLOCKS_TABLE_NAME} b 
            LEFT JOIN (SELECT block_id, JSON_AGG(JSON_BUILD_OBJECT('hash_id', encode(it.hash, 'hex'), 'wrapper_id', encode(it.wrapper_id, 'hex'), 'code_type', it.code_type)) 
            AS inner_txs FROM {0}.{INNER_TX_TABLE_NAME} it GROUP BY it.block_id) it ON b.block_id = it.block_id 
            LEFT JOIN (SELECT block_id, JSON_AGG(JSON_BUILD_OBJECT('hash_id', encode(wt.hash, 'hex'))) 
            AS wrapper_txs FROM {0}.{WRAPPER_TX_TABLE_NAME} wt GROUP BY wt.block_id) wt ON b.block_id = wt.block_id 
            WHERE b.header_height = $1;", self.network);

        query(&str)
            .bind(block_height as i32)
            .fetch_optional(&*self.pool)
            .await
            .map_err(Error::from)
    }

    #[instrument(skip(self))]
    /// Returns the latest block, otherwise returns an Error.
    pub async fn get_last_block(&self) -> Result<Row, Error> {
        let str = format!("SELECT b.*, inner_txs, wrapper_txs FROM {0}.{BLOCKS_TABLE_NAME} b 
            LEFT JOIN (SELECT block_id, JSON_AGG(JSON_BUILD_OBJECT('hash_id', encode(it.hash, 'hex'), 'wrapper_id', encode(it.wrapper_id, 'hex'), 'code_type', it.code_type)) 
            AS inner_txs FROM {0}.{INNER_TX_TABLE_NAME} it GROUP BY it.block_id) it ON b.block_id = it.block_id 
            LEFT JOIN (SELECT block_id, JSON_AGG(JSON_BUILD_OBJECT('hash_id', encode(wt.hash, 'hex'))) 
            AS wrapper_txs FROM {0}.{WRAPPER_TX_TABLE_NAME} wt GROUP BY wt.block_id) wt ON b.block_id = wt.block_id 
            WHERE b.header_height = (SELECT MAX(header_height) FROM {0}.{BLOCKS_TABLE_NAME});", self.network);
        // use query_one as the row matching max height is unique.
        query(&str)
            .fetch_one(&*self.pool)
            .await
            .map_err(Error::from)
    }

    #[instrument(skip(self))]
    /// Returns the latest height value, otherwise returns an Error.
    pub async fn get_last_height(&self) -> Result<Row, Error> {
        let str = format!(
            "SELECT MAX(header_height) AS header_height FROM {}.{BLOCKS_TABLE_NAME}",
            self.network
        );

        // use query_one as the row matching max height is unique.
        query(&str)
            .fetch_one(&*self.pool)
            .await
            .map_err(Error::from)
    }

    #[instrument(skip(self))]
    /// Returns inner transaction identified by hash
    pub async fn get_tx(&self, hash: &[u8]) -> Result<Option<Row>, Error> {
        // query for transaction with hash
        let str = format!(
            "SELECT * FROM {}.{INNER_TX_TABLE_NAME} WHERE hash=$1",
            self.network
        );

        query(&str)
            .bind(hash)
            .fetch_optional(&*self.pool)
            .await
            .map_err(Error::from)
    }

    #[instrument(skip(self))]
    /// Returns transfer transactions for a given source
    pub async fn get_txs_by_address(&self, address: &String) -> Result<Vec<Row>, Error> {
        // query for transaction with hash
        let str = format!(
            "SELECT * FROM {}.tx_transfer WHERE source = $1 OR target = $1;",
            self.network
        );

        query(&str)
            .bind(address)
            .fetch_all(&*self.pool)
            .await
            .map_err(Error::from)
    }

    #[instrument(skip(self))]
    /// Returns all the inner tx hashes for a block
    pub async fn get_tx_hashes_block(&self, hash: &[u8]) -> Result<Vec<Row>, Error> {
        // query for all tx hash that are in a block identified by the block_id
        let str = format!("SELECT t.hash, t.code_type FROM {0}.{BLOCKS_TABLE_NAME} b JOIN {0}.{INNER_TX_TABLE_NAME} t ON b.block_id = t.block_id WHERE b.block_id = $1;", self.network);

        query(&str)
            .bind(hash)
            .fetch_all(&*self.pool)
            .await
            .map_err(Error::from)
    }

    #[instrument(skip(self))]
    /// Returns Shielded transactions
    pub async fn get_shielded_tx(&self) -> Result<Vec<Row>, Error> {
        // query for transaction with hash
        let str = format!(
            "SELECT * FROM {}.tx_transfer WHERE source = '{MASP_ADDR}' OR target = '{MASP_ADDR}';",
            self.network
        );

        query(&str)
            .fetch_all(&*self.pool)
            .await
            .map_err(Error::from)
    }

    #[instrument(skip(self))]
    /// Returns info about the indexes existing on the table, otherwise returns an Error.
    pub async fn check_indexes(&self) -> Result<Vec<Row>, Error> {
        let str = format!(
            "SELECT indexname, indexdef FROM pg_indexes WHERE tablename = '{BLOCKS_TABLE_NAME}';"
        );

        query(&str)
            .fetch_all(&*self.pool)
            .await
            .map_err(Error::from)
    }

    /// Retrieves a historical list of thresholds associated with a given account.
    ///
    /// This function executes a SQL query to aggregate thresholds (`ARRAY_AGG`) for the specified
    /// `account_id`. The thresholds are ordered by `update_id`, which serves as a chronological marker,
    /// indicating the sequence of updates. The most recent threshold is at the end of the list.
    ///
    /// # Parameters
    ///
    /// - `account_id`: A string slice (`&str`) representing the unique identifier of the account.
    ///
    /// # Returns
    ///
    /// - On success, returns an `Option<Row>`. The `Row` contains an aggregated list
    ///   of thresholds (aliased as `thresholds`) for the account. If `account_id` does not exists
    ///   this will return Ok(None), otherwise Ok(Some(Row)) is returned, containing lists
    ///   of all thresholds associated with that account, or an empty lists if no threshold updates
    ///   have happend.
    /// - On failure, returns an `Error`.
    ///
    /// # Usage
    ///
    /// This function is useful for tracking the evolution of thresholds associated with an account over time.
    /// It provides a comprehensive history, allowing users or systems to understand how the thresholds
    /// associated with the account have changed and to identify the current threshold in use.
    pub async fn account_thresholds(&self, account_id: &str) -> Result<Option<Row>, Error> {
        // NOTE: there are two scenarios:
        // - account_id does not exists in such case this query will return Ok(None), because we
        // use query.fetch_optional()
        let to_query = format!(
            "
            SELECT COALESCE(ARRAY_AGG(data->>'threshold' ORDER BY data->>'addr' ASC), ARRAY[]::text[]) AS thresholds
            FROM {}.inner_transactions
            WHERE code = '\\x70f91d4f778d05d40c5a56490ced906b016e4b7a2a2ef5ff0ac0541ff28c5a22' AND data->>'addr' = $1 GROUP BY data->>'addr';
            ",
            self.network
        );

        query(&to_query)
            .bind(account_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(Error::from)
    }

    /// Retrieves a historical list of vp_code_hashes associated with a given account.
    ///
    /// This function executes a SQL query to aggregate vp_code_hashes (`ARRAY_AGG`) for the specified
    /// `account_id`. The hashes are ordered by `update_id`, which serves as a chronological marker,
    /// indicating the sequence of updates. The most recent hash is at the end of the list.
    ///
    /// # Parameters
    ///
    /// - `account_id`: A string slice (`&str`) representing the unique identifier of the account.
    ///
    /// # Returns
    ///
    /// - On success, returns an `Option<Row>`. The `Row` contains an aggregated list
    ///   of vp_code_hashes (aliased as `code_hashes`) for the account. if `account_id` does not exists,
    ///   it returns `Ok(None)`.
    /// - On failure, returns an `Error`.
    ///
    /// # Usage
    ///
    /// This function is useful for tracking the evolution of vp_code_hashes associated with an account over time.
    /// It provides a comprehensive history, allowing users or systems to understand how the vp_code_hashes
    /// associated with the account have changed and to identify the current vp_code_hash in use.
    pub async fn account_vp_codes(&self, account_id: &str) -> Result<Option<Row>, Error> {
        // NOTE: there are two scenarios:
        // - account_id does not exists in such case this query will return Ok(None), because we
        // use query.fetch_optional()
        // - There are not updates including vp_code_hashe so far, in that case we use
        // COALESCE which return a [] empty list instead of null.
        let to_query = format!(
            "
            SELECT COALESCE(ARRAY_AGG(data->>'vp_code_hash' ORDER BY data->>'addr' ASC), ARRAY[]::text[]) AS code_hashes
            FROM {}.account_updates
            WHERE code = '\\x70f91d4f778d05d40c5a56490ced906b016e4b7a2a2ef5ff0ac0541ff28c5a22' AND data->>'addr' = $1 GROUP BY data->>'addr';
            ",
            self.network
        );

        query(&to_query)
            .bind(account_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(Error::from)
    }

    /// Retrieves a historical list of public key sets associated with a given account.
    ///
    /// This function executes a SQL query to aggregate public keys (`ARRAY_AGG`) for each `update_id`
    /// associated with the specified `account_id`. The keys within each batch are ordered by their `id`.
    /// The `update_id` serves as a chronological marker, indicating when each set of public keys was
    /// associated with the account. The most recent set is at the end of the list.
    ///
    /// # Parameters
    ///
    /// - `account_id`: A string slice (`&str`) representing the unique identifier of the account.
    ///
    /// # Returns
    /// - On success, returns Ok(None) if there is no account_id or public_keys associated to it.
    ///   otherwise Ok(Some(Row)) containing the lists of public_keys_batches associated to this
    ///   account.
    /// - An `Error` on failure
    ///
    /// # Details
    ///
    /// - The function groups (`GROUP BY`) the public keys based on the `update_id` and orders (`ORDER BY`)
    ///   the overall result set in ascending order of `update_id`.
    /// - Each `Row` in the returned vector represents a different update to the account, containing a
    ///   batch of public keys. These batches are ordered chronologically, with the last element in the
    ///   vector representing the most recent set of public keys associated with the account.
    ///
    /// # Usage
    ///
    /// This function is useful for tracking the evolution of public keys associated with an account over time.
    /// It provides a comprehensive history, allowing users or systems to understand how the account's
    /// public keys have changed and to identify the current set of public keys.
    pub async fn account_public_keys(&self, account_id: &str) -> Result<Vec<Row>, Error> {
        let to_query = format!(
            "
            SELECT ARRAY_AGG(data->>'public_keys')
            FROM {}.inner_transactions
            WHERE code = '\\x70f91d4f778d05d40c5a56490ced906b016e4b7a2a2ef5ff0ac0541ff28c5a22' AND data->>'addr' = $1;
        ",
            self.network,
        );

        // Each returned row would contain a vector of public keys formatted as strings.
        // The column's name is publick_key_batch.
        query(&to_query)
            .bind(account_id)
            .fetch_all(&*self.pool)
            .await
            .map_err(Error::from)
    }

    pub async fn vote_proposal_data(&self, proposal_id: i64) -> Result<Vec<Row>, Error> {
        let query = format!(
            "SELECT data FROM {}.inner_transactions WHERE code_type = 'tx_vote_proposal' AND (data->>'id')::int = $1;",
            self.network
        );

        // Execute the query and fetch the first row (if any)
        sqlx::query(&query)
            .bind(proposal_id)
            .fetch_all(&*self.pool)
            .await
            .map_err(Error::from)
    }

    // Return the number of commits signed by the `validator_address` in a range of 500 blocks.
    // It is use to calculate the validator uptime.
    pub async fn validator_uptime(
        &self,
        validator_address: &[u8],
        start: Option<&i32>,
        end: Option<&i32>,
    ) -> Result<Row, Error> {
        // if no parameters defined we return result on the last 500 blocks
        let mut q = format!(
            "SELECT COUNT(*)
                FROM {0}.commit_signatures
                WHERE validator_address = $1
                AND block_id IN
                    (SELECT block_id FROM {0}.blocks WHERE header_height BETWEEN (SELECT MAX(header_height) FROM {0}.blocks) - 499 AND (SELECT MAX(header_height) FROM {0}.blocks))",
            self.network,
        );

        if start.is_some() && end.is_some() {
            q = format!(
                "SELECT COUNT(*)
                    FROM {0}.commit_signatures
                    WHERE validator_address = $1
                    AND block_id IN
                        (SELECT block_id FROM {0}.blocks WHERE header_height BETWEEN ($2 + 1) AND $3)",
                self.network,
            );
        }

        query(&q)
            .bind(validator_address)
            .bind(start)
            .bind(end)
            .fetch_one(&*self.pool)
            .await
            .map_err(Error::from)
    }

    #[instrument(skip(self))]
    /// Returns the latest block, otherwise returns an Error.
    pub async fn get_lastest_blocks(
        &self,
        num: &i32,
        offset: Option<&i32>,
    ) -> Result<Vec<Row>, Error> {
        let str = format!("SELECT b.*, t.txs FROM {0}.blocks b LEFT JOIN (SELECT block_id, JSON_AGG(JSON_BUILD_OBJECT('hash_id', encode(t.hash, 'hex'), 'code_type', t.code_type)) AS txs FROM {0}.inner_transactions t GROUP BY t.block_id) t ON b.block_id = t.block_id ORDER BY b.header_height DESC LIMIT {1} OFFSET {2};", self.network, num, offset.unwrap_or(&  0));

        // use query_one as the row matching max height is unique.
        query(&str)
            .fetch_all(&*self.pool)
            .await
            .map_err(Error::from)
    }

    pub fn pool(&self) -> &PgPool {
        self.pool.as_ref()
    }
}
