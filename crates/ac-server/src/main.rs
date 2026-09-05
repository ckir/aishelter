use ac_server::config::Settings;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let settings = Settings::new()
        .unwrap_or_else(|_| Settings::default());

    tracing::info!("Agent Commons starting on {}:{}", settings.host, settings.port);

    // TODO: wire routes and start server
    tokio::signal::ctrl_c().await?;
    Ok(())
}
