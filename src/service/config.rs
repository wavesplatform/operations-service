//! Operation services' config.

use serde::Deserialize;
use thiserror::Error;

use crate::common::database::config::PostgresConfig;

#[derive(Clone)]
pub struct ServiceConfig {
    /// Server port
    pub port: u16,

    /// Postgres database config
    pub db: PostgresConfig,

    /// Database pool size
    pub db_pool_size: u32,
}

#[derive(Deserialize)]
struct RawConfig {
    /// Server port
    #[serde(rename = "port", default = "default_port")]
    port: u16,

    /// Database pool size
    #[serde(rename = "pgpoolsize", default = "default_db_pool_size")]
    pub db_pool_size: u32,
}

fn default_port() -> u16 {
    8080
}

fn default_db_pool_size() -> u32 {
    8
}

#[derive(Error, Debug)]
#[error("configuration error: {0}")]
pub struct ConfigError(#[from] envy::Error);

pub fn load() -> Result<ServiceConfig, ConfigError> {
    let raw_config = envy::from_env::<RawConfig>()?;
    let pg_config = envy::from_env::<PostgresConfig>()?;

    let config = ServiceConfig {
        port: raw_config.port,
        db: pg_config,
        db_pool_size: raw_config.db_pool_size,
    };

    Ok(config)
}
