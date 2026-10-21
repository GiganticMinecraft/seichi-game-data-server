use domain::app_models::{QueryTimeoutError, VecDataSource};
use domain::models::{
    Player, PlayerBreakCount, PlayerBuildCount, PlayerLastQuit, PlayerPlayTicks, PlayerVoteCount,
};

use config::SourceDatabaseConfig;

use anyhow::{anyhow, ensure};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::mysql::{MySqlPoolOptions, MySqlRow};
use sqlx::query::Map;
use sqlx::{IntoArguments, MySql, Pool, Row};
use std::time::Duration;

/// プールから接続を取り出すまでの待ち時間の上限。
///
/// sqlx の既定値は 30 秒だが、全接続が埋まっている状態で 30 秒も待たせると
/// 呼び出し元 (publisher はリクエストタイムアウト 30 秒) が先に諦めてしまい、
/// 失敗の原因がサーバー側で見えなくなる。接続の確立も含めてこの時間で打ち切る。
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

async fn create_mysql_connection_pool(
    config: &SourceDatabaseConfig,
) -> Result<Pool<MySql>, anyhow::Error> {
    ensure!(
        config.pool_max_connections > 0,
        "DB_POOL_MAX_CONNECTIONS must be greater than 0"
    );

    Ok(MySqlPoolOptions::new()
        .max_connections(config.pool_max_connections)
        .acquire_timeout(ACQUIRE_TIMEOUT)
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
    query_timeout: Duration,
}

impl MySqlDataSource {
    /// `query` を実行して全行を取得する。
    ///
    /// 接続の取得は `ACQUIRE_TIMEOUT`、クエリの実行は `query_timeout` で打ち切る。
    /// クエリが時間切れになった場合は [`QueryTimeoutError`] を返す。
    async fn fetch_all_with_timeout<F, O, A>(
        &self,
        query: Map<'static, MySql, F, A>,
    ) -> anyhow::Result<Vec<O>>
    where
        F: FnMut(MySqlRow) -> Result<O, sqlx::Error> + Send + 'static,
        O: Send + Unpin + 'static,
        A: 'static + Send + IntoArguments<MySql>,
    {
        let mut connection = self.connection_pool.acquire().await?;

        match tokio::time::timeout(self.query_timeout, query.fetch_all(&mut *connection)).await {
            Ok(result) => result.map_err(|e| anyhow!(e)),
            Err(_elapsed) => {
                // 打ち切った時点ではクエリの結果がまだ読み終わっておらず、接続は
                // 中途半端な状態にある。プールに戻すと sqlx が on-release の ping で
                // 結果を読み切るまで待ってしまうので、戻さずに閉じる。
                connection.close_on_drop();
                Err(QueryTimeoutError {
                    timeout: self.query_timeout,
                }
                .into())
            }
        }
    }
}

// 利用するゲームDBのテーブル定義は
// https://github.com/GiganticMinecraft/SeichiAssist/blob/2994a7269edb0427bd9d59c8ec822742638609c2/src/main/resources/db/migration/V1.0.0__Create_static_tables_and_columns.sql
// を参照されたい。

#[async_trait]
impl VecDataSource<PlayerLastQuit> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerLastQuit>> {
        let query =
            sqlx::query::<MySql>("SELECT name, uuid, lastquit From playerdata").try_map(|row| {
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
            });
        self.fetch_all_with_timeout(query).await
    }
}

#[async_trait]
impl VecDataSource<PlayerBreakCount> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerBreakCount>> {
        let query = sqlx::query::<MySql>("SELECT name, uuid, totalbreaknum From playerdata")
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
            });
        self.fetch_all_with_timeout(query).await
    }
}

#[async_trait]
impl VecDataSource<PlayerBuildCount> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerBuildCount>> {
        let query =
            sqlx::query::<MySql>("SELECT name, uuid, build_count From playerdata").try_map(|row| {
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
            });
        self.fetch_all_with_timeout(query).await
    }
}

#[async_trait]
impl VecDataSource<PlayerPlayTicks> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerPlayTicks>> {
        let query =
            sqlx::query::<MySql>("SELECT name, uuid, playtick From playerdata").try_map(|row| {
                Ok(PlayerPlayTicks {
                    player: Player {
                        // varchar(128) -> String
                        uuid: row.try_get("uuid")?,
                        // varchar(30) -> String
                        last_known_name: row.try_get("name")?,
                    },
                    // i64 -> u64
                    play_ticks: row.try_get::<i64, _>("playtick")? as u64,
                })
            });
        self.fetch_all_with_timeout(query).await
    }
}

#[async_trait]
impl VecDataSource<PlayerVoteCount> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerVoteCount>> {
        let query = sqlx::query::<MySql>("SELECT playerdata.name, playerdata.uuid, vote_number From vote INNER JOIN playerdata ON vote.uuid = playerdata.uuid")
            .try_map(|row| {
                Ok(PlayerVoteCount {
                    player: Player {
                        // varchar(128) -> String
                        uuid: row.try_get("uuid")?,
                        // varchar(30) -> String
                        last_known_name: row.try_get("name")?,
                    },
                    // i32 -> u64
                    vote_count: row.try_get::<i32, _>("vote_number")? as u64,
                })
            })
        ;
        self.fetch_all_with_timeout(query).await
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
    ensure!(
        config.query_timeout_secs > 0,
        "DB_QUERY_TIMEOUT_SECS must be greater than 0"
    );

    let connection_pool = create_mysql_connection_pool(config).await?;
    Ok(MySqlDataSource {
        connection_pool,
        query_timeout: Duration::from_secs(config.query_timeout_secs),
    })
}
