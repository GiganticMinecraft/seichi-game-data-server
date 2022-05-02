use crate::app_models::VecDataSource;
use crate::gigantic_minecraft::game_data;
use crate::gigantic_minecraft::game_data::v1::read_service_server::ReadService;
use crate::gigantic_minecraft::game_data::v1::{
    break_counts_response, build_counts_response, last_quits_response, play_ticks_response,
    vote_counts_response, BreakCountsResponse, BuildCountsResponse, LastQuitsResponse,
    PlayTicksResponse, VoteCountsResponse,
};
use crate::models::{
    Player, PlayerBreakCount, PlayerBuildCount, PlayerLastQuit, PlayerPlayTicks, PlayerVoteCount,
};
use async_trait::async_trait;

fn to_tonic_player(model: Player) -> game_data::v1::Player {
    game_data::v1::Player {
        uuid: model.uuid,
        last_known_name: model.last_known_name,
    }
}

fn to_tonic_last_quit_response(model: Vec<PlayerLastQuit>) -> tonic::Response<LastQuitsResponse> {
    tonic::Response::new(LastQuitsResponse {
        results: model
            .into_iter()
            .map(|last_quit| last_quits_response::PlayerLastQuit {
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
            .map(|break_count| break_counts_response::PlayerBreakCount {
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
            .map(|build_count| build_counts_response::PlayerBuildCount {
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
            .map(|play_ticks| play_ticks_response::PlayerPlayTicks {
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
            .map(|vote_count| vote_counts_response::PlayerVoteCount {
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

pub struct ReadServiceImpl<LastQuitDS, BreakCountDS, BuildCountDS, PlayTicksDS, VoteCountDS> {
    pub last_quit_data_source: LastQuitDS,
    pub break_counts_data_source: BreakCountDS,
    pub build_counts_data_source: BuildCountDS,
    pub play_ticks_data_source: PlayTicksDS,
    pub vote_counts_data_source: VoteCountDS,
}

#[async_trait]
impl<LastQuitDS, BreakCountDS, BuildCountDS, PlayTicksDS, VoteCountDS> ReadService
    for ReadServiceImpl<LastQuitDS, BreakCountDS, BuildCountDS, PlayTicksDS, VoteCountDS>
where
    LastQuitDS: VecDataSource<PlayerLastQuit> + Send + Sync + 'static,
    BreakCountDS: VecDataSource<PlayerBreakCount> + Send + Sync + 'static,
    BuildCountDS: VecDataSource<PlayerBuildCount> + Send + Sync + 'static,
    PlayTicksDS: VecDataSource<PlayerPlayTicks> + Send + Sync + 'static,
    VoteCountDS: VecDataSource<PlayerVoteCount> + Send + Sync + 'static,
{
    async fn last_quits(
        &self,
        _request: tonic::Request<::pbjson_types::Empty>,
    ) -> Result<tonic::Response<LastQuitsResponse>, tonic::Status> {
        self.last_quit_data_source
            .fetch()
            .await
            .map(to_tonic_last_quit_response)
            .map_err(to_tonic_error_status)
    }

    async fn break_counts(
        &self,
        _request: tonic::Request<::pbjson_types::Empty>,
    ) -> Result<tonic::Response<BreakCountsResponse>, tonic::Status> {
        self.break_counts_data_source
            .fetch()
            .await
            .map(to_tonic_break_counts_response)
            .map_err(to_tonic_error_status)
    }

    async fn build_counts(
        &self,
        _request: tonic::Request<::pbjson_types::Empty>,
    ) -> Result<tonic::Response<BuildCountsResponse>, tonic::Status> {
        self.build_counts_data_source
            .fetch()
            .await
            .map(to_tonic_build_counts_response)
            .map_err(to_tonic_error_status)
    }

    async fn play_ticks(
        &self,
        _request: tonic::Request<::pbjson_types::Empty>,
    ) -> Result<tonic::Response<PlayTicksResponse>, tonic::Status> {
        self.play_ticks_data_source
            .fetch()
            .await
            .map(to_tonic_play_ticks_response)
            .map_err(to_tonic_error_status)
    }

    async fn vote_counts(
        &self,
        _request: tonic::Request<::pbjson_types::Empty>,
    ) -> Result<tonic::Response<VoteCountsResponse>, tonic::Status> {
        self.vote_counts_data_source
            .fetch()
            .await
            .map(to_tonic_vote_counts_response)
            .map_err(to_tonic_error_status)
    }
}
