use crate::app_models::VecDataSource;
use crate::config::SourceDatabaseConfig;
use crate::models::{
    Player, PlayerBreakCount, PlayerBuildCount, PlayerLastQuit, PlayerPlayTicks, PlayerVoteCount,
};
use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::mysql::{MySqlPoolOptions, MySqlRow};
use sqlx::{Error, FromRow, MySql, Pool, Row};

async fn create_connection_pool(config: &SourceDatabaseConfig) -> Result<Pool<MySql>, sqlx::Error> {
    const MAX_CONNS: u32 = 5;

    MySqlPoolOptions::new()
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
        .await
}

struct MySqlDataSource {
    connection_pool: Pool<MySql>,
}

// 利用するゲームDBのテーブル定義は
// https://github.com/GiganticMinecraft/SeichiAssist/blob/2994a7269edb0427bd9d59c8ec822742638609c2/src/main/resources/db/migration/V1.0.0__Create_static_tables_and_columns.sql
// を参照されたい。

impl <'r> FromRow<'r, MySqlRow> for PlayerLastQuit {
    fn from_row(row: &'r MySqlRow) -> Result<Self, Error> {
        Ok(
            PlayerLastQuit {
                player: Player {
                    // varchar(128) -> String
                    uuid: row.try_get("uuid")?,
                    // varchar(30) -> String
                    last_known_name: row.try_get("name")?,
                },
                // datetime -> String
                rfc_3339_date_time: row.try_get::<DateTime<Utc>, _>("lastquit")?.to_rfc3339(),
            }
        )
    }
}

#[async_trait]
impl VecDataSource<PlayerLastQuit> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerLastQuit>> {
        sqlx::query_as("SELECT name, uuid, lastquit From playerdata")
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}

impl <'r> FromRow<'r, MySqlRow> for PlayerBreakCount {
    fn from_row(row: &'r MySqlRow) -> Result<Self, Error> {
        Ok(
            PlayerBreakCount {
                player: Player {
                    // varchar(128) -> String
                    uuid: row.try_get("uuid")?,
                    // varchar(30) -> String
                    last_known_name: row.try_get("name")?,
                },
                // bigint -> String
                break_count: row.try_get("totalbreaknum")?,
            }
        )
    }
}

#[async_trait]
impl VecDataSource<PlayerBreakCount> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerBreakCount>> {
        sqlx::query_as("SELECT name, uuid, totalbreaknum From playerdata")
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}

impl <'r> FromRow<'r, MySqlRow> for PlayerBuildCount {
    fn from_row(row: &'r MySqlRow) -> Result<Self, Error> {
        Ok(
            PlayerBuildCount {
                player: Player {
                    // varchar(128) -> String
                    uuid: row.try_get("uuid")?,
                    // varchar(30) -> String
                    last_known_name: row.try_get("name")?,
                },
                // double -> u64
                build_count: row.try_get::<f64, _>("build_count")?.round() as u64,
            }
        )
    }
}

#[async_trait]
impl VecDataSource<PlayerBuildCount> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerBuildCount>> {
        sqlx::query_as("SELECT name, uuid, build_count From playerdata")
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}

impl <'r> FromRow<'r, MySqlRow> for PlayerPlayTicks {
    fn from_row(row: &'r MySqlRow) -> Result<Self, Error> {
        Ok(
            PlayerPlayTicks {
                player: Player {
                    // varchar(128) -> String
                    uuid: row.try_get("uuid")?,
                    // varchar(30) -> String
                    last_known_name: row.try_get("name")?,
                },
                // i32 -> u64
                play_ticks: row.try_get::<i32, _>("playtick")? as u64,
            }
        )
    }
}

#[async_trait]
impl VecDataSource<PlayerPlayTicks> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerPlayTicks>> {
        sqlx::query_as("SELECT name, uuid, playtick From playerdata")
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}

impl <'r> FromRow<'r, MySqlRow> for PlayerVoteCount {
    fn from_row(row: &'r MySqlRow) -> Result<Self, Error> {
        Ok(
            PlayerVoteCount {
                player: Player {
                    // varchar(128) -> String
                    uuid: row.try_get("uuid")?,
                    // varchar(30) -> String
                    last_known_name: row.try_get("name")?,
                },
                // i32 -> u64
                vote_count: row.try_get::<i32, _>("p_vote")? as u64,
            }
        )
    }
}

#[async_trait]
impl VecDataSource<PlayerVoteCount> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerVoteCount>> {
        sqlx::query_as("SELECT name, uuid, p_vote From playerdata")
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}


pub async fn last_quit_data_source(
    config: &SourceDatabaseConfig,
) -> Result<impl VecDataSource<PlayerLastQuit> + Send + Sync + 'static, sqlx::Error> {
    let connection_pool = create_connection_pool(config).await?;
    Ok(MySqlDataSource { connection_pool })
}

pub async fn break_count_data_source(
    config: &SourceDatabaseConfig,
) -> Result<impl VecDataSource<PlayerBreakCount> + Send + Sync + 'static, sqlx::Error> {
    let connection_pool = create_connection_pool(config).await?;
    Ok(MySqlDataSource { connection_pool })
}

pub async fn build_count_data_source(
    config: &SourceDatabaseConfig,
) -> Result<impl VecDataSource<PlayerBuildCount> + Send + Sync + 'static, sqlx::Error> {
    let connection_pool = create_connection_pool(config).await?;
    Ok(MySqlDataSource { connection_pool })
}

pub async fn play_ticks_data_source(
    config: &SourceDatabaseConfig,
) -> Result<impl VecDataSource<PlayerPlayTicks> + Send + Sync + 'static, sqlx::Error> {
    let connection_pool = create_connection_pool(config).await?;
    Ok(MySqlDataSource { connection_pool })
}

pub async fn vote_count_data_source(
    config: &SourceDatabaseConfig,
) -> Result<impl VecDataSource<PlayerVoteCount> + Send + Sync + 'static, sqlx::Error> {
    let connection_pool = create_connection_pool(config).await?;
    Ok(MySqlDataSource { connection_pool })
}
