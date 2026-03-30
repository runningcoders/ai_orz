mod error;
mod handlers;
mod models;
mod pkg;
mod router;
mod service;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let _db = pkg::storage::init_from_config("ai_orz.toml")?;
    tracing::info!("Database initialized: data/ai_orz.db");

    service::init();
    tracing::info!("Service layer initialized");

    let app = router::create_router();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Server listening on 0.0.0.0:3000");

    axum::serve(listener, app).await?;

    Ok(())
}
