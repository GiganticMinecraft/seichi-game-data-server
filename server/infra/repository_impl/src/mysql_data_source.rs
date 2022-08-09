use domain::app_models::VecDataSource;
use domain::models::{
    Player, PlayerBreakCount, PlayerBuildCount, PlayerLastQuit, PlayerPlayTicks, PlayerVoteCount,
};

use config::SourceDatabaseConfig;

use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{MySql, Pool, Row};

async fn create_mysql_connection_pool(
    config: &SourceDatabaseConfig,
) -> Result<Pool<MySql>, anyhow::Error> {
    const MAX_CONNS: u32 = 5;

    Ok(MySqlPoolOptions::new()
        .max_connections(MAX_CONNS)
        .connect(
            format!(
                "mysql://{user}:{pass}@{host}:{port}/{db}",
                user = config.user,
                pass = config.password,
                host = config.host,
                port = config.port.0,
                db = config.database_name
            )
            .as_str(),
        )
        .await?)
}

#[derive(Debug, Clone)]
struct MySqlDataSource {
    connection_pool: Pool<MySql>,
}

// 利用するゲームDBのテーブル定義は
// https://github.com/GiganticMinecraft/SeichiAssist/blob/2994a7269edb0427bd9d59c8ec822742638609c2/src/main/resources/db/migration/V1.0.0__Create_static_tables_and_columns.sql
// を参照されたい。

#[async_trait]
impl VecDataSource<PlayerLastQuit> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerLastQuit>> {
        sqlx::query::<MySql>("SELECT name, uuid, lastquit From playerdata")
            .try_map(|row| {
                Ok(PlayerLastQuit {
                    player: Player {
                        // varchar(128) -> String
                        uuid: row.try_get("uuid")?,
                        // varchar(30) -> String
                        last_known_name: row.try_get("name")?,
                    },
                    // datetime -> String
                    rfc_3339_date_time: row.try_get::<DateTime<Utc>, _>("lastquit")?.to_rfc3339(),
                })
            })
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}

#[async_trait]
impl VecDataSource<PlayerBreakCount> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerBreakCount>> {
        sqlx::query::<MySql>("SELECT name, uuid, totalbreaknum From playerdata")
            .try_map(|row| {
                Ok(PlayerBreakCount {
                    player: Player {
                        // varchar(128) -> String
                        uuid: row.try_get("uuid")?,
                        // varchar(30) -> String
                        last_known_name: row.try_get("name")?,
                    },
                    // bigint -> i64 -> u64
                    // because bigint corresponds to i64 (https://docs.rs/sqlx/0.6.1/sqlx/mysql/types/index.html)
                    break_count: row.try_get::<i64, _>("totalbreaknum")? as u64,
                })
            })
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}

#[async_trait]
impl VecDataSource<PlayerBuildCount> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerBuildCount>> {
        sqlx::query::<MySql>("SELECT name, uuid, build_count From playerdata")
            .try_map(|row| {
                Ok(PlayerBuildCount {
                    player: Player {
                        // varchar(128) -> String
                        uuid: row.try_get("uuid")?,
                        // varchar(30) -> String
                        last_known_name: row.try_get("name")?,
                    },
                    // double -> u64
                    build_count: row.try_get::<f64, _>("build_count")?.round() as u64,
                })
            })
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}

#[async_trait]
impl VecDataSource<PlayerPlayTicks> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerPlayTicks>> {
        sqlx::query::<MySql>("SELECT name, uuid, playtick From playerdata")
            .try_map(|row| {
                Ok(PlayerPlayTicks {
                    player: Player {
                        // varchar(128) -> String
                        uuid: row.try_get("uuid")?,
                        // varchar(30) -> String
                        last_known_name: row.try_get("name")?,
                    },
                    // i32 -> u64
                    play_ticks: row.try_get::<i32, _>("playtick")? as u64,
                })
            })
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}

#[async_trait]
impl VecDataSource<PlayerVoteCount> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerVoteCount>> {
        sqlx::query::<MySql>("SELECT name, uuid, p_vote From playerdata")
            .try_map(|row| {
                Ok(PlayerVoteCount {
                    player: Player {
                        // varchar(128) -> String
                        uuid: row.try_get("uuid")?,
                        // varchar(30) -> String
                        last_known_name: row.try_get("name")?,
                    },
                    // i32 -> u64
                    vote_count: row.try_get::<i32, _>("p_vote")? as u64,
                })
            })
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}

pub trait CombinedDataSource:
    VecDataSource<PlayerLastQuit>
    + VecDataSource<PlayerBreakCount>
    + VecDataSource<PlayerBuildCount>
    + VecDataSource<PlayerPlayTicks>
    + VecDataSource<PlayerVoteCount>
    + Clone
    + Send
    + Sync
    + 'static
{
}

impl CombinedDataSource for MySqlDataSource {}

pub async fn from_config(config: &SourceDatabaseConfig) -> anyhow::Result<impl CombinedDataSource> {
    let connection_pool = create_mysql_connection_pool(config).await?;
    Ok(MySqlDataSource { connection_pool })
}
