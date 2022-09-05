//! Operations service.

use std::sync::Arc;

mod config;
mod db;
mod repo;
mod server;

pub async fn main() -> Result<(), anyhow::Error> {
    // Load configs
    let config = config::load()?;
    let port = config.port;

    // Create repo
    log::info!("Connecting to database: {:?}", config.db);
    let pgpool = db::pool::new(&config.db, config.db_pool_size)?;
    let repo = repo::postgres::PgRepo::new(pgpool);

    // Create the web server
    let server = server::ServerBuilder::new().repo(repo).build().new_server();

    // Run the web server
    Arc::new(server).run(port).await;

    Ok(())
}
