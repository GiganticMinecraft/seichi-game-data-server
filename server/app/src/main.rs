#![deny(clippy::all)]
#![warn(clippy::nursery, clippy::pedantic, clippy::cargo)]
#![allow(clippy::cargo_common_metadata)]

use config::{AppConfig, FromEnv, SourceDatabaseConfig};
use infra_grpc::buf_generated::gigantic_minecraft::seichi_game_data::v1::read_service_server::{
    ReadService, ReadServiceServer,
};
use infra_grpc::read_service::ReadServiceImpl;
use tonic::transport::Server;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

async fn initialize_database_read_service(
    config: &SourceDatabaseConfig,
) -> anyhow::Result<impl ReadService> {
    use infra_repository_impl::mysql_data_source;

    let data_source = Box::new(mysql_data_source::from_config(config).await?);

    Ok(ReadServiceImpl {
        last_quit_data_source: data_source.clone(),
        break_counts_data_source: data_source.clone(),
        build_counts_data_source: data_source.clone(),
        play_ticks_data_source: data_source.clone(),
        vote_counts_data_source: data_source,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // initialize tracing
    // see https://github.com/tokio-rs/axum/blob/79a0a54bc9f0f585c974b5e6793541baff980662/examples/tracing-aka-logging/src/main.rs
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    println!("Reading config...");
    let config = AppConfig::from_env()?;

    let service = initialize_database_read_service(&config.source_database_config)
        .await
        .expect("Initializing read service");

    let serve_address = format!("{}:{}", config.http_config.host, config.http_config.port.0)
        .parse()
        .expect("Parsing serve address from config");

    println!("Server will be listening on {serve_address}");

    Server::builder()
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .add_service(ReadServiceServer::new(service))
        .serve(serve_address)
        .await?;

    Ok(())
}
