use crate::buf_generated::gigantic_minecraft::seichi_game_data;
use crate::buf_generated::gigantic_minecraft::seichi_game_data::v1::{
    read_service_server::ReadService, BreakCountsResponse, BuildCountsResponse, LastQuitsResponse,
    PlayTicksResponse, VoteCountsResponse,
};
use async_trait::async_trait;
use domain::app_models::VecDataSource;
use domain::models::{
    Player, PlayerBreakCount, PlayerBuildCount, PlayerLastQuit, PlayerPlayTicks, PlayerVoteCount,
};

fn to_tonic_player(model: Player) -> seichi_game_data::v1::Player {
    seichi_game_data::v1::Player {
        uuid: model.uuid,
        last_known_name: model.last_known_name,
    }
}

fn to_tonic_last_quit_response(model: Vec<PlayerLastQuit>) -> tonic::Response<LastQuitsResponse> {
    tonic::Response::new(LastQuitsResponse {
        results: model
            .into_iter()
            .map(|last_quit| seichi_game_data::v1::PlayerLastQuit {
                player: Some(to_tonic_player(last_quit.player)),
                rfc_3339_date_time: last_quit.rfc_3339_date_time,
            })
            .collect(),
    })
}

fn to_tonic_break_counts_response(
    model: Vec<PlayerBreakCount>,
) -> tonic::Response<BreakCountsResponse> {
    tonic::Response::new(BreakCountsResponse {
        results: model
            .into_iter()
            .map(|break_count| seichi_game_data::v1::PlayerBreakCount {
                player: Some(to_tonic_player(break_count.player)),
                break_count: break_count.break_count,
            })
            .collect(),
    })
}

fn to_tonic_build_counts_response(
    model: Vec<PlayerBuildCount>,
) -> tonic::Response<BuildCountsResponse> {
    tonic::Response::new(BuildCountsResponse {
        results: model
            .into_iter()
            .map(|build_count| seichi_game_data::v1::PlayerBuildCount {
                player: Some(to_tonic_player(build_count.player)),
                build_count: build_count.build_count,
            })
            .collect(),
    })
}

fn to_tonic_play_ticks_response(model: Vec<PlayerPlayTicks>) -> tonic::Response<PlayTicksResponse> {
    tonic::Response::new(PlayTicksResponse {
        results: model
            .into_iter()
            .map(|play_ticks| seichi_game_data::v1::PlayerPlayTicks {
                player: Some(to_tonic_player(play_ticks.player)),
                play_ticks: play_ticks.play_ticks,
            })
            .collect(),
    })
}

fn to_tonic_vote_counts_response(
    model: Vec<PlayerVoteCount>,
) -> tonic::Response<VoteCountsResponse> {
    tonic::Response::new(VoteCountsResponse {
        results: model
            .into_iter()
            .map(|vote_count| seichi_game_data::v1::PlayerVoteCount {
                player: Some(to_tonic_player(vote_count.player)),
                vote_count: vote_count.vote_count,
            })
            .collect(),
    })
}

fn to_tonic_error_status(anyhow_error: anyhow::Error) -> tonic::Status {
    use tonic::*;

    log::error!("Received an error from data source: {}", anyhow_error);

    Status::unknown("Unknown error. See the server log for more details.")
}

pub struct ReadServiceImpl {
    pub last_quit_data_source: Box<dyn VecDataSource<PlayerLastQuit> + Send + Sync>,
    pub break_counts_data_source: Box<dyn VecDataSource<PlayerBreakCount> + Send + Sync>,
    pub build_counts_data_source: Box<dyn VecDataSource<PlayerBuildCount> + Send + Sync>,
    pub play_ticks_data_source: Box<dyn VecDataSource<PlayerPlayTicks> + Send + Sync>,
    pub vote_counts_data_source: Box<dyn VecDataSource<PlayerVoteCount> + Send + Sync>,
}

#[async_trait]
impl ReadService for ReadServiceImpl {
    async fn last_quits(
        &self,
        _request: tonic::Request<pbjson_types::Empty>,
    ) -> Result<tonic::Response<LastQuitsResponse>, tonic::Status> {
        self.last_quit_data_source
            .fetch()
            .await
            .map(to_tonic_last_quit_response)
            .map_err(to_tonic_error_status)
    }

    async fn break_counts(
        &self,
        _request: tonic::Request<pbjson_types::Empty>,
    ) -> Result<tonic::Response<BreakCountsResponse>, tonic::Status> {
        self.break_counts_data_source
            .fetch()
            .await
            .map(to_tonic_break_counts_response)
            .map_err(to_tonic_error_status)
    }

    async fn build_counts(
        &self,
        _request: tonic::Request<pbjson_types::Empty>,
    ) -> Result<tonic::Response<BuildCountsResponse>, tonic::Status> {
        self.build_counts_data_source
            .fetch()
            .await
            .map(to_tonic_build_counts_response)
            .map_err(to_tonic_error_status)
    }

    async fn play_ticks(
        &self,
        _request: tonic::Request<pbjson_types::Empty>,
    ) -> Result<tonic::Response<PlayTicksResponse>, tonic::Status> {
        self.play_ticks_data_source
            .fetch()
            .await
            .map(to_tonic_play_ticks_response)
            .map_err(to_tonic_error_status)
    }

    async fn vote_counts(
        &self,
        _request: tonic::Request<pbjson_types::Empty>,
    ) -> Result<tonic::Response<VoteCountsResponse>, tonic::Status> {
        self.vote_counts_data_source
            .fetch()
            .await
            .map(to_tonic_vote_counts_response)
            .map_err(to_tonic_error_status)
    }
}
