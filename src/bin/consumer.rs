//! Operations consumer.

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    lib::consumer::main().await
}
