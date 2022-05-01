use crate::gigantic_minecraft::game_data::v1::read_service_server::ReadService;
use crate::gigantic_minecraft::game_data::v1::{
    BreakCountsResponse, BuildCountsResponse, LastQuitsResponse, PlayTicksResponse,
    VoteCountsResponse,
};

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
