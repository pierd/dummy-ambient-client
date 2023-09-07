
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let host = std::env::args().skip(1).next().unwrap_or_else(|| "localhost".to_string());
    dummy_ambient_client::run(host).await
}
