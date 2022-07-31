#![deny(clippy::all, clippy::cargo)]
#![warn(clippy::nursery, clippy::pedantic)]
#![allow(clippy::cargo_common_metadata)]

use config::{AppConfig, FromEnv, SourceDatabaseConfig};
use infra_grpc::buf_generated::gigantic_minecraft::seichi_game_data::v1::read_service_server::{
    ReadService, ReadServiceServer,
};
use infra_grpc::read_service::ReadServiceImpl;
use tonic::transport::Server;

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
    println!("Reading config...");
    let config = AppConfig::from_env()?;

    let service = initialize_database_read_service(&config.source_database_config)
        .await
        .expect("Initializing read service");

    let serve_address = format!("{}:{}", config.http_config.host, config.http_config.port.0)
        .parse()
        .expect("Parsing serve address from config");

    println!("Server will be listening on {}", serve_address);

    Server::builder()
        .add_service(ReadServiceServer::new(service))
        .serve(serve_address)
        .await?;

    Ok(())
}
