//! Operations service.

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    lib::service::main().await
}
