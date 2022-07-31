#[derive(Debug, Clone)]
pub struct Player {
    pub uuid: String,
    pub last_known_name: String,
}

#[derive(Debug, Clone)]
pub struct PlayerLastQuit {
    pub player: Player,
    pub rfc_3339_date_time: String,
}

#[derive(Debug, Clone)]
pub struct PlayerBreakCount {
    pub player: Player,
    pub break_count: u64,
}

#[derive(Debug, Clone)]
pub struct PlayerBuildCount {
    pub player: Player,
    pub build_count: u64,
}

#[derive(Debug, Clone)]
pub struct PlayerPlayTicks {
    pub player: Player,
    pub play_ticks: u64,
}

#[derive(Debug, Clone)]
pub struct PlayerVoteCount {
    pub player: Player,
    pub vote_count: u64,
}
