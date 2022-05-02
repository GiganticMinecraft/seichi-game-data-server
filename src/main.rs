#![deny(clippy::all, clippy::cargo)]
#![warn(clippy::nursery, clippy::pedantic)]
#![allow(clippy::cargo_common_metadata)]

use seichi_game_api::config::{Config, FromEnv, SourceDatabaseConfig};
use seichi_game_api::data_sources;
use seichi_game_api::gigantic_minecraft::seichi_game_data::v1::read_service_server::{
    ReadService, ReadServiceServer,
};
use seichi_game_api::services::read::ReadServiceImpl;
use tonic::transport::Server;

async fn initialize_read_service(config: &SourceDatabaseConfig) -> impl ReadService {
    let expect_message = "Initializing data source";

    let last_quit_data_source = data_sources::last_quit_data_source(config)
        .await
        .expect(expect_message);
    let break_counts_data_source = data_sources::break_count_data_source(config)
        .await
        .expect(expect_message);
    let build_counts_data_source = data_sources::build_count_data_source(config)
        .await
        .expect(expect_message);
    let play_ticks_data_source = data_sources::play_ticks_data_source(config)
        .await
        .expect(expect_message);
    let vote_counts_data_source = data_sources::vote_count_data_source(config)
        .await
        .expect(expect_message);

    ReadServiceImpl {
        last_quit_data_source,
        break_counts_data_source,
        build_counts_data_source,
        play_ticks_data_source,
        vote_counts_data_source,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Reading config...");
    let config = Config::from_env()?;

    let service = initialize_read_service(&config.source_database_config).await;

    let serve_address = format!("{}:{}", config.http_config.host, config.http_config.port.0)
        .parse()
        .unwrap();

    println!("Server listening on {}", serve_address);

    Server::builder()
        .add_service(ReadServiceServer::new(service))
        .serve(serve_address)
        .await?;

    Ok(())
}
