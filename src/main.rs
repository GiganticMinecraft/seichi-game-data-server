#![deny(clippy::all, clippy::cargo)]
#![warn(clippy::nursery, clippy::pedantic)]
#![allow(clippy::cargo_common_metadata)]

use seichi_game_api::config::{Config, FromEnv};
use seichi_game_api::gigantic_minecraft::game_data::v1::read_service_server::ReadServiceServer;
use seichi_game_api::services::read::ReadServiceImpl;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Reading config...");
    let config = Config::from_env()?;

    let service = ReadServiceImpl::default();

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
