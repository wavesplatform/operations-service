//! Database connections

pub mod pool {
    //! Pooled connections to the database

    use deadpool_diesel::postgres::{Manager, Pool, Runtime};

    use crate::common::database::config::PostgresConfig;

    pub type PgPool = Pool;

    pub fn new(config: &PostgresConfig, pool_size: u32) -> Result<PgPool, anyhow::Error> {
        let db_url = config.database_url();
        let manager = Manager::new(db_url, Runtime::Tokio1);
        let pool = Pool::builder(manager).max_size(pool_size as usize).build()?;
        Ok(pool)
    }
}
