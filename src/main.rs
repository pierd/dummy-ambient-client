#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dummy_ambient_client::logging::init_subscriber(
        "dummy_ambient_client".into(),
        "info".into(),
        std::io::stdout,
    );
    let host = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "localhost".to_string());
    dummy_ambient_client::run(host).await
}
