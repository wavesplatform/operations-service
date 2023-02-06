//! Operations consumer.

mod batcher;
mod config;
mod metrics;
mod model;
mod storage;
mod updates;

pub async fn main() -> Result<(), anyhow::Error> {
    let config = config::load()?;
    consumer::run(config).await
}

#[allow(clippy::module_inception)]
mod consumer {
    use std::time::Instant;

    use diesel::{pg::PgConnection, Connection};
    use tokio::task;

    use wx_warp::endpoints::MetricsWarpBuilder;

    use crate::consumer::batcher;
    use crate::consumer::config::ConsumerConfig;
    use crate::consumer::metrics::{DB_WRITE_TIME, HEIGHT, UPDATES_BATCH_SIZE, UPDATES_BATCH_TIME};
    use crate::consumer::storage::{PostgresStorage, Repo, Storage};
    use crate::consumer::updates::{BlockchainUpdate, BlockchainUpdates, BlockchainUpdatesSource};

    pub(super) async fn run(config: ConsumerConfig) -> anyhow::Result<()> {
        // Initialize connection to the database and fetch latest height
        let init_db_task = task::spawn(async move {
            log::info!("Connecting to database: {:?}", config.db);
            let conn = PgConnection::establish(&config.db.database_url())?;
            let storage = PostgresStorage::new(conn);
            let last_height = storage
                .transaction(|repo| {
                    let last_height = repo.last_height()?;
                    log::info!("Last height stored in database is {:?}", last_height);
                    let rollback_to_height = last_height.and_then(|h| if h > 1 { Some(h - 1) } else { None });
                    if let Some(height) = rollback_to_height {
                        repo.rollback_to_height(height)?;
                        log::info!("Rolled back to height {} for safety", height);
                    }
                    Ok(last_height)
                })
                .await?;
            Ok::<_, anyhow::Error>((storage, last_height))
        });

        let init_updates_task = task::spawn(async move {
            let url = config.blockchain_updates.blockchain_updates_url;
            log::info!("Connecting to blockchain-updates at {}", url);
            BlockchainUpdates::connect(url).await
        });

        let (storage, last_processed_height) = init_db_task.await??;
        let updates_source = init_updates_task.await??;

        let metrics_port = config.metrics_port;
        task::spawn(async move {
            if let Some(height) = last_processed_height {
                HEIGHT.set(height as i64);
            }
            MetricsWarpBuilder::new()
                .with_metric(&*HEIGHT)
                .with_metric(&*UPDATES_BATCH_SIZE)
                .with_metric(&*UPDATES_BATCH_TIME)
                .with_metric(&*DB_WRITE_TIME)
                .with_metrics_port(metrics_port)
                .run_async()
                .await;
        });

        let starting_height = last_processed_height.unwrap_or(config.blockchain_updates.starting_height);
        log::info!("Starting to fetch updates from height {}", starting_height);

        let rx = updates_source.stream(starting_height).await?;
        let mut rx = batcher::start(rx, config.batching);
        let mut last_height = starting_height;
        while let Some(updates) = rx.recv().await {
            let count = updates.len();
            let start = Instant::now();
            log::debug!("Writing batch of {} updates", count);
            let new_last_height = write_batch(updates, storage.clone()).await?;
            last_height = new_last_height.unwrap_or(last_height);
            let elapsed = start.elapsed();
            log::info!(
                "Saved {} updates in {:?}, last height is {}",
                count,
                elapsed,
                last_height
            );
        }
        Ok(())
    }

    async fn write_batch(batch: Vec<BlockchainUpdate>, storage: impl Storage) -> anyhow::Result<Option<u32>> {
        storage
            .transaction(|repo| {
                let start = Instant::now();
                let mut last_height = None;
                for update in batch {
                    match update {
                        BlockchainUpdate::Append(append) => {
                            let block_id = append.block_id;
                            let block_height = append.height;
                            let block_timestamp = append.timestamp.expect("block timestamp");
                            let block_uid = repo.insert_block(&block_id, block_height, block_timestamp)?;
                            for tx in append.transactions {
                                let tx_id = tx.id.as_str();
                                let tx_type = tx.tx_type as u8;
                                let sender = tx.sender.as_str();
                                let tx_body = serde_json::to_value(&tx)?;
                                //log::trace!("tx_json = {}", tx_body.to_string());
                                repo.insert_tx(tx_id, block_uid, sender, tx_type, tx_body)?;
                            }
                            last_height = Some(append.height);
                        }
                        BlockchainUpdate::Rollback(rollback) => {
                            let block_uid = repo.block_uid(&rollback.block_id)?;
                            repo.rollback_to_block(block_uid)?;
                        }
                    }
                }
                let elapsed = start.elapsed();
                let elapsed_ms = elapsed.as_millis() as i64;
                DB_WRITE_TIME.set(elapsed_ms);
                if let Some(height) = last_height {
                    HEIGHT.set(height as i64);
                }
                Ok(last_height)
            })
            .await
    }
}
