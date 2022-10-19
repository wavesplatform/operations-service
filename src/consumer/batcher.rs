//! Batcher for the BlockchainUpdate stream.
//!
//! Handles batching of sequential blocks or microblocks
//! as well as short rollbacks (within internal buffer size).
//! Longer rollbacks must be handled by the database writer.
//!
//! Always introduces a delay of 1 microblock
//! to handle the most common rollback type in-memory.

use std::time::{Duration, Instant};

use itertools::Itertools;
use tokio::{sync::mpsc, task};

use crate::consumer::metrics::{UPDATES_BATCH_SIZE, UPDATES_BATCH_TIME};
use crate::consumer::updates::BlockchainUpdate;

#[derive(Clone, Default)]
pub struct BatchingParams {
    pub max_updates: Option<usize>,
    pub max_delay: Option<Duration>,
}

pub fn start(
    input: mpsc::Receiver<BlockchainUpdate>,
    batching_params: BatchingParams,
) -> mpsc::Receiver<Vec<BlockchainUpdate>> {
    let (tx, rx) = mpsc::channel::<Vec<BlockchainUpdate>>(1);
    let buffer_capacity = batching_params.max_updates.unwrap_or(1);
    let mut batcher = Batcher {
        input,
        output: tx,
        batching_params,
        buffer: Vec::with_capacity(buffer_capacity),
        last_block_timestamp: None,
        last_block_height: None,
        last_flush: Instant::now(),
    };
    task::spawn(async move {
        batcher.run().await.expect("receiver dropped");
    });
    rx
}

struct Batcher {
    input: mpsc::Receiver<BlockchainUpdate>,
    output: mpsc::Sender<Vec<BlockchainUpdate>>,
    batching_params: BatchingParams,
    buffer: Vec<BlockchainUpdate>,
    last_block_timestamp: Option<u64>,
    last_block_height: Option<u32>,
    last_flush: Instant,
}

impl Batcher {
    async fn run(&mut self) -> Result<(), mpsc::error::SendError<Vec<BlockchainUpdate>>> {
        while let Some(update) = self.input.recv().await {
            self.push_update(update);
            if self.need_flush() {
                let count = self.buffer.len();
                let time = self.last_flush.elapsed();
                log::debug!("Collected {} updates in {:?}", count, time,);
                UPDATES_BATCH_SIZE.set(count as i64);
                UPDATES_BATCH_TIME.set(time.as_millis() as i64);
                self.flush().await?;
            }
        }
        Ok(())
    }

    fn push_update(&mut self, mut update: BlockchainUpdate) {
        match update {
            BlockchainUpdate::Append(ref mut append) => {
                // Propagate timestamp from the last known block at the same height to the microblock
                if append.is_microblock && append.timestamp.is_none() {
                    if let Some(last_height) = self.last_block_height {
                        if last_height == append.height {
                            assert!(
                                self.last_block_timestamp.is_some(),
                                "Internal error: propagate timestamp failed (no saved timestamp)"
                            );
                            append.timestamp = self.last_block_timestamp;
                        } else {
                            panic!(
                                "Internal error: propagate timestamp failed (last_height={}, append.height={})",
                                last_height, append.height
                            );
                        }
                    } else {
                        panic!("Internal error: propagate timestamp failed (no known block)");
                    }
                } else {
                    self.last_block_height = Some(append.height);
                    self.last_block_timestamp = append.timestamp;
                }
                self.buffer.push(update);
            }
            BlockchainUpdate::Rollback(ref rollback) => {
                // Scan buffer backwards until we find the required block.
                // If found - remove all updates after it and discard this rollback,
                // otherwise just put this rollback to the buffer
                // so this rollback will be handled by the database.
                for (i, item) in self.buffer.iter().enumerate().rev() {
                    if let BlockchainUpdate::Append(append) = item {
                        if append.block_id == rollback.block_id {
                            let i = i + 1; // Drop starting from the next update
                            self.buffer.drain(i..);
                            return; // Discard the rollback itself - we've already handled it
                        }
                    }
                }
                self.last_block_height = None;
                self.last_block_timestamp = None;
                self.buffer.push(update); // Let database handle the rollback
            }
        }
    }

    fn need_flush(&self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }

        // Don't flush if there is a rollback on top, wait for the replacement block
        if let Some(BlockchainUpdate::Rollback(_)) = self.buffer.last() {
            return false;
        }

        // Flush if there are rollbacks in the queue, but not on top (already have a replacement block)
        if self.buffer.iter().any(|u| matches!(u, BlockchainUpdate::Rollback(_))) {
            return true;
        }

        // Flush if there is a microblock on top + some more updates below it
        // (don't flush if there is only one microblock - to handle most common 1-mb rollback)
        if self.buffer.len() > 1 {
            if let Some(BlockchainUpdate::Append(last_append)) = self.buffer.last() {
                if last_append.is_microblock {
                    return true;
                }
            }
        }

        // FLush if there are enough updates in the buffer
        if let Some(max_updates) = self.batching_params.max_updates {
            if self.buffer.len() >= max_updates {
                return true;
            }
        }

        // Flush if max_delay exceeded since last flush
        if let Some(max_delay) = self.batching_params.max_delay {
            if self.last_flush.elapsed() >= max_delay {
                return true;
            }
        }

        // Don't buffer if not batching parameters set (or we risk to stash forever...)
        if self.batching_params.max_updates.is_none() && self.batching_params.max_delay.is_none() {
            return true;
        }

        false
    }

    async fn flush(&mut self) -> Result<(), mpsc::error::SendError<Vec<BlockchainUpdate>>> {
        let mut delayed_update = None;
        if let Some(BlockchainUpdate::Append(append)) = self.buffer.last() {
            if append.is_microblock {
                delayed_update = self.buffer.pop();
                debug_assert!(delayed_update.is_some());
            }
        }
        let updates = self.buffer.drain(..).collect_vec();
        self.output.send(updates).await?;
        if let Some(delayed_update) = delayed_update {
            self.buffer.push(delayed_update);
        }
        self.last_flush = Instant::now();
        Ok(())
    }
}
