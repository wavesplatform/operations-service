//! Operation services consumer's config.

use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

use crate::common::database::config::PostgresConfig;
use crate::consumer::batcher::BatchingParams;

#[derive(Clone)]
pub struct ConsumerConfig {
    /// Blockchain updates config
    pub blockchain_updates: BlockchainUpdatesConfig,

    /// Postgres database config
    pub db: PostgresConfig,

    /// Batching of the database writes
    pub batching: BatchingParams,
}

#[derive(Clone)]
pub struct BlockchainUpdatesConfig {
    /// Blockchain updates service URL
    pub blockchain_updates_url: String,

    /// TCP Keepalive for the GRPC connection
    pub tcp_keepalive: Duration,

    /// Listen to blockchain updates starting from this blockchain height
    pub starting_height: u32,
}

#[derive(Deserialize)]
struct BlockchainUpdatesRawConfig {
    /// Blockchain updates service URL
    #[serde(rename = "blockchain_updates_url")]
    pub blockchain_updates_url: String,

    /// TCP Keepalive (seconds) for the GRPC connection
    #[serde(rename = "starting_height", default = "default_tcp_keepalive_secs")]
    pub tcp_keepalive_secs: u32,

    /// Listen to blockchain updates starting from this blockchain height
    #[serde(rename = "starting_height", default = "default_starting_height")]
    pub starting_height: u32,
}

fn default_starting_height() -> u32 {
    0
}

fn default_tcp_keepalive_secs() -> u32 {
    5
}

#[derive(Deserialize)]
struct BatchingRawConfig {
    #[serde(rename = "batch_max_size", default = "default_batch_max_size")]
    batch_max_size: u32,
    #[serde(rename = "batch_max_delay_sec", default = "default_batch_max_delay_sec")]
    batch_max_delay_sec: u32,
}

fn default_batch_max_size() -> u32 {
    256
}

fn default_batch_max_delay_sec() -> u32 {
    10
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("configuration error: {0}")]
    EnvyError(#[from] envy::Error),

    #[error("configuration error: invalid {0} parameter: {1}")]
    ValidationError(&'static str, &'static str),
}

pub fn load() -> Result<ConsumerConfig, ConfigError> {
    let blockchain_updates_config = envy::from_env::<BlockchainUpdatesRawConfig>()?;
    let pg_config = envy::from_env::<PostgresConfig>()?;
    let batch_config = envy::from_env::<BatchingRawConfig>()?;

    // Need this because later we are gonna cast it to i32
    if blockchain_updates_config.starting_height > i32::MAX as u32 {
        return Err(ConfigError::ValidationError("STARTING_HEIGHT", "value is too big"));
    }

    let config = ConsumerConfig {
        blockchain_updates: BlockchainUpdatesConfig {
            blockchain_updates_url: blockchain_updates_config.blockchain_updates_url,
            tcp_keepalive: Duration::from_secs(blockchain_updates_config.tcp_keepalive_secs as u64),
            starting_height: blockchain_updates_config.starting_height,
        },
        db: pg_config,
        batching: BatchingParams {
            max_updates: Some(batch_config.batch_max_size as usize),
            max_delay: Some(Duration::from_secs(batch_config.batch_max_delay_sec as u64)),
        },
    };

    Ok(config)
}
