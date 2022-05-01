#![deny(clippy::all, clippy::cargo)]
#![warn(clippy::nursery, clippy::pedantic)]
#![allow(clippy::cargo_common_metadata)]

use seichi_game_api::config::{Config, FromEnv};
use seichi_game_api::gigantic_minecraft::game_data::v1::read_service_server::{
    ReadService, ReadServiceServer,
};
use seichi_game_api::gigantic_minecraft::game_data::v1::{
    BreakCountsResponse, BuildCountsResponse, LastQuitsResponse, PlayTicksResponse,
    VoteCountsResponse,
};
use tonic::transport::Server;

#[derive(Default)]
pub struct ReadServiceImpl {}

#[tonic::async_trait]
impl ReadService for ReadServiceImpl {
    async fn last_quits(
        &self,
        _request: tonic::Request<::pbjson_types::Empty>,
    ) -> Result<tonic::Response<LastQuitsResponse>, tonic::Status> {
        todo!()
    }

    async fn break_counts(
        &self,
        _request: tonic::Request<::pbjson_types::Empty>,
    ) -> Result<tonic::Response<BreakCountsResponse>, tonic::Status> {
        todo!()
    }

    async fn build_counts(
        &self,
        _request: tonic::Request<::pbjson_types::Empty>,
    ) -> Result<tonic::Response<BuildCountsResponse>, tonic::Status> {
        todo!()
    }

    async fn play_ticks(
        &self,
        _request: tonic::Request<::pbjson_types::Empty>,
    ) -> Result<tonic::Response<PlayTicksResponse>, tonic::Status> {
        todo!()
    }

    async fn vote_counts(
        &self,
        _request: tonic::Request<::pbjson_types::Empty>,
    ) -> Result<tonic::Response<VoteCountsResponse>, tonic::Status> {
        todo!()
    }
}

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
