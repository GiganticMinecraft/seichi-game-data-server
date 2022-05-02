use crate::app_models::VecDataSource;
use crate::config::SourceDatabaseConfig;
use crate::models::{
    Player, PlayerBreakCount, PlayerBuildCount, PlayerLastQuit, PlayerPlayTicks, PlayerVoteCount,
};
use anyhow::anyhow;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::mysql::{MySqlPoolOptions, MySqlRow};
use sqlx::{MySql, Pool, Row};

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

struct DataSourceImpl {
    connection_pool: Pool<MySql>,
}

// VecDataSourceの実装。
//
// 利用するゲームDBのテーブル定義は
// https://github.com/GiganticMinecraft/SeichiAssist/blob/2994a7269edb0427bd9d59c8ec822742638609c2/src/main/resources/db/migration/V1.0.0__Create_static_tables_and_columns.sql
// を参照されたい。

#[async_trait]
impl VecDataSource<PlayerLastQuit> for DataSourceImpl {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerLastQuit>> {
        sqlx::query("SELECT name, uuid, last_quit From playerdata")
            .map(|row: MySqlRow| {
                PlayerLastQuit {
                    player: Player {
                        // varchar(128) -> String
                        uuid: row.get("uuid"),
                        // varchar(30) -> String
                        last_known_name: row.get("name"),
                    },
                    // datetime -> String
                    rfc_3339_date_time: row.get::<DateTime<Utc>, _>("lastquit").to_rfc3339(),
                }
            })
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}

#[async_trait]
impl VecDataSource<PlayerBreakCount> for DataSourceImpl {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerBreakCount>> {
        sqlx::query("SELECT name, uuid, totalbreaknum From playerdata")
            .map(|row: MySqlRow| {
                PlayerBreakCount {
                    player: Player {
                        // varchar(128) -> String
                        uuid: row.get("uuid"),
                        // varchar(30) -> String
                        last_known_name: row.get("name"),
                    },
                    // bigint -> String
                    break_count: row.get("totalbreaknum"),
                }
            })
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}

#[async_trait]
impl VecDataSource<PlayerBuildCount> for DataSourceImpl {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerBuildCount>> {
        sqlx::query("SELECT name, uuid, build_count From playerdata")
            .map(|row: MySqlRow| {
                PlayerBuildCount {
                    player: Player {
                        // varchar(128) -> String
                        uuid: row.get("uuid"),
                        // varchar(30) -> String
                        last_known_name: row.get("name"),
                    },
                    // double -> u64
                    build_count: row.get::<f64, _>("build_count").round() as u64,
                }
            })
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}

#[async_trait]
impl VecDataSource<PlayerPlayTicks> for DataSourceImpl {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerPlayTicks>> {
        sqlx::query("SELECT name, uuid, playtick From playerdata")
            .map(|row: MySqlRow| {
                PlayerPlayTicks {
                    player: Player {
                        // varchar(128) -> String
                        uuid: row.get("uuid"),
                        // varchar(30) -> String
                        last_known_name: row.get("name"),
                    },
                    // i32 -> u64
                    play_ticks: row.get::<i32, _>("playtick") as u64,
                }
            })
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}

#[async_trait]
impl VecDataSource<PlayerVoteCount> for DataSourceImpl {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerVoteCount>> {
        sqlx::query("SELECT name, uuid, p_vote From playerdata")
            .map(|row: MySqlRow| {
                PlayerVoteCount {
                    player: Player {
                        // varchar(128) -> String
                        uuid: row.get("uuid"),
                        // varchar(30) -> String
                        last_known_name: row.get("name"),
                    },
                    // i32 -> u64
                    vote_count: row.get::<i32, _>("p_vote") as u64,
                }
            })
            .fetch_all(&self.connection_pool)
            .await
            .map_err(|e| anyhow!(e))
    }
}

pub async fn last_quit_data_source(
    config: &SourceDatabaseConfig,
) -> Result<impl VecDataSource<PlayerLastQuit> + Send + Sync + 'static, sqlx::Error> {
    let connection_pool = create_connection_pool(config).await?;
    Ok(DataSourceImpl { connection_pool })
}

pub async fn break_count_data_source(
    config: &SourceDatabaseConfig,
) -> Result<impl VecDataSource<PlayerBreakCount> + Send + Sync + 'static, sqlx::Error> {
    let connection_pool = create_connection_pool(config).await?;
    Ok(DataSourceImpl { connection_pool })
}

pub async fn build_count_data_source(
    config: &SourceDatabaseConfig,
) -> Result<impl VecDataSource<PlayerBuildCount> + Send + Sync + 'static, sqlx::Error> {
    let connection_pool = create_connection_pool(config).await?;
    Ok(DataSourceImpl { connection_pool })
}

pub async fn play_ticks_data_source(
    config: &SourceDatabaseConfig,
) -> Result<impl VecDataSource<PlayerPlayTicks> + Send + Sync + 'static, sqlx::Error> {
    let connection_pool = create_connection_pool(config).await?;
    Ok(DataSourceImpl { connection_pool })
}

pub async fn vote_count_data_source(
    config: &SourceDatabaseConfig,
) -> Result<impl VecDataSource<PlayerVoteCount> + Send + Sync + 'static, sqlx::Error> {
    let connection_pool = create_connection_pool(config).await?;
    Ok(DataSourceImpl { connection_pool })
}
