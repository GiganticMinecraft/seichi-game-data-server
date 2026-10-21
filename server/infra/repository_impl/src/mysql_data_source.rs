use domain::app_models::{QueryTimeoutError, VecDataSource};
use domain::models::{
    Player, PlayerBreakCount, PlayerBuildCount, PlayerLastQuit, PlayerPlayTicks, PlayerVoteCount,
};

use config::SourceDatabaseConfig;

use anyhow::{anyhow, ensure};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::mysql::{MySqlPoolOptions, MySqlRow};
use sqlx::pool::PoolConnection;
use sqlx::query::Map;
use sqlx::{IntoArguments, MySql, Pool, Row};
use std::time::{Duration, Instant};
use tracing::Instrument;
use tracing::field::Empty;

use crate::metrics::{DbOperationMetrics, sqlx_error_type};

/// プールから接続を取り出すまでの待ち時間の上限。
///
/// sqlx の既定値は 30 秒だが、全接続が埋まっている状態で 30 秒も待たせると
/// 呼び出し元 (publisher はリクエストタイムアウト 30 秒) が先に諦めてしまい、
/// 失敗の原因がサーバー側で見えなくなる。接続の確立も含めてこの時間で打ち切る。
///
/// sqlx はこの値を起動時の最初の接続 (`connect`) の待ち時間にも使うので、
/// 起動時に DB へ 5 秒以内につながらなければプロセスはエラーで終了する
/// (Kubernetes が再起動する)。
pub const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

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

/// 実行する SQL と、span / メトリクスに載せるその要約。
struct Statement {
    /// 実行する SQL。パラメータを持たない固定文字列なので、そのまま
    /// `db.query.text` として span に載せてよい。
    sql: &'static str,
    /// `db.query.summary`。span 名にも使う (`{operation} {table}` の形)。
    summary: &'static str,
    /// `db.collection.name`。JOIN のように対象テーブルが 1 つに決まらない
    /// クエリでは `None` にする (semantic conventions の要件)。
    collection: Option<&'static str>,
}

impl Statement {
    /// `playerdata` テーブルだけを読む SELECT。
    const fn select_playerdata(sql: &'static str) -> Self {
        Self {
            sql,
            summary: "SELECT playerdata",
            collection: Some("playerdata"),
        }
    }
}

#[derive(Debug, Clone)]
struct MySqlDataSource {
    connection_pool: Pool<MySql>,
    query_timeout: Duration,
    /// `db.namespace` / `server.address` / `server.port` として span に載せる。
    database_name: String,
    server_address: String,
    server_port: u16,
    metrics: DbOperationMetrics,
}

impl MySqlDataSource {
    /// `statement` を実行して全行を取得する。
    ///
    /// 接続の取得は `ACQUIRE_TIMEOUT`、クエリの実行は `query_timeout` で打ち切る。
    /// クエリが時間切れになった場合は [`QueryTimeoutError`] を返す。
    ///
    /// 接続の取得からクエリの完了までを OpenTelemetry の client span
    /// (呼び出し元の gRPC server span の子) で囲み、所要時間を
    /// `db.client.operation.duration` に記録する。
    async fn fetch_all_with_timeout<F, O, A>(
        &self,
        statement: &Statement,
        query: Map<'static, MySql, F, A>,
    ) -> anyhow::Result<Vec<O>>
    where
        F: FnMut(MySqlRow) -> Result<O, sqlx::Error> + Send + 'static,
        O: Send + Unpin + 'static,
        A: 'static + Send + IntoArguments<MySql>,
    {
        let span = tracing::info_span!(
            "db.query",
            otel.name = statement.summary,
            otel.kind = "client",
            otel.status_code = Empty,
            otel.status_description = Empty,
            // `db.system` は旧 semantic conventions の名前だが、Tempo の
            // service graph が DB を仮想ノードとして扱うのに使うので残す。
            db.system = "mysql",
            db.system.name = "mysql",
            db.namespace = %self.database_name,
            db.collection.name = statement.collection,
            db.operation.name = "SELECT",
            db.query.summary = statement.summary,
            db.query.text = statement.sql,
            server.address = %self.server_address,
            server.port = i64::from(self.server_port),
            db.response.returned_rows = Empty,
            error.type = Empty,
        );

        let operation = DbOperation {
            span: span.clone(),
            start: Instant::now(),
            metrics: &self.metrics,
            collection: statement.collection,
            finished: false,
        };
        let result = self.fetch_all_inner(query).instrument(span).await;
        operation.finish(&result);

        result
    }

    async fn fetch_all_inner<F, O, A>(
        &self,
        query: Map<'static, MySql, F, A>,
    ) -> anyhow::Result<Vec<O>>
    where
        F: FnMut(MySqlRow) -> Result<O, sqlx::Error> + Send + 'static,
        O: Send + Unpin + 'static,
        A: 'static + Send + IntoArguments<MySql>,
    {
        let mut connection = CloseUnlessReleased(Some(self.connection_pool.acquire().await?));
        let conn = connection.0.as_mut().expect("connection is set above");

        match tokio::time::timeout(self.query_timeout, query.fetch_all(&mut **conn)).await {
            Ok(Ok(rows)) => {
                connection.release();
                Ok(rows)
            }
            Ok(Err(e)) => Err(anyhow!(e)),
            Err(_elapsed) => Err(QueryTimeoutError {
                timeout: self.query_timeout,
            }
            .into()),
        }
    }
}

/// 1 回の DB 操作の span とメトリクスの後始末。
///
/// 完了したら [`DbOperation::finish`] で結果を記録する。呼び出し元の future
/// ごと drop された場合 (クライアントの `grpc-timeout` や切断、サーバーの
/// `REQUEST_TIMEOUT` による打ち切り) は `Drop` で `error.type = "cancelled"`
/// として記録する。
struct DbOperation<'a> {
    span: tracing::Span,
    start: Instant,
    metrics: &'a DbOperationMetrics,
    collection: Option<&'static str>,
    finished: bool,
}

impl DbOperation<'_> {
    fn finish<T>(mut self, result: &anyhow::Result<Vec<T>>) {
        self.finished = true;
        match result {
            Ok(rows) => {
                self.span.record(
                    "db.response.returned_rows",
                    i64::try_from(rows.len()).unwrap_or(i64::MAX),
                );
                self.record(None);
            }
            Err(e) => {
                let error_type = if e.is::<QueryTimeoutError>() {
                    "timeout"
                } else {
                    e.downcast_ref::<sqlx::Error>()
                        .map_or("_OTHER", sqlx_error_type)
                };
                self.span.record("otel.status_description", e.to_string());
                self.record(Some(error_type));
            }
        }
    }

    fn record(&self, error_type: Option<&'static str>) {
        if let Some(error_type) = error_type {
            self.span.record("otel.status_code", "ERROR");
            self.span.record("error.type", error_type);
        }
        self.metrics
            .record(self.start.elapsed(), self.collection, error_type);
    }
}

impl Drop for DbOperation<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.record(Some("cancelled"));
        }
    }
}

/// クエリが最後まで成功したときだけ接続をプールに戻し、それ以外では閉じる。
///
/// クエリの途中で打ち切られた接続 (このモジュールのタイムアウトに加えて、
/// クライアントの `grpc-timeout`・サーバーの `REQUEST_TIMEOUT`・ストリームの
/// リセットで future ごと drop された場合も含む) は結果を読み切っていない。
/// そのままプールに戻すと、sqlx が on-release の ping で残りの結果を読み切る
/// までその接続を使えなくなり、プールが埋まる。なので戻さずに閉じる。
struct CloseUnlessReleased(Option<PoolConnection<MySql>>);

impl CloseUnlessReleased {
    fn release(&mut self) {
        // drop するとプールに戻る。
        self.0.take();
    }
}

impl Drop for CloseUnlessReleased {
    fn drop(&mut self) {
        if let Some(connection) = self.0.as_mut() {
            connection.close_on_drop();
        }
    }
}

// 利用するゲームDBのテーブル定義は
// https://github.com/GiganticMinecraft/SeichiAssist/blob/2994a7269edb0427bd9d59c8ec822742638609c2/src/main/resources/db/migration/V1.0.0__Create_static_tables_and_columns.sql
// を参照されたい。

#[async_trait]
impl VecDataSource<PlayerLastQuit> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerLastQuit>> {
        const STATEMENT: Statement =
            Statement::select_playerdata("SELECT name, uuid, lastquit From playerdata");

        let query = sqlx::query::<MySql>(STATEMENT.sql).try_map(|row| {
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
        self.fetch_all_with_timeout(&STATEMENT, query).await
    }
}

#[async_trait]
impl VecDataSource<PlayerBreakCount> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerBreakCount>> {
        const STATEMENT: Statement =
            Statement::select_playerdata("SELECT name, uuid, totalbreaknum From playerdata");

        let query = sqlx::query::<MySql>(STATEMENT.sql).try_map(|row| {
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
        self.fetch_all_with_timeout(&STATEMENT, query).await
    }
}

#[async_trait]
impl VecDataSource<PlayerBuildCount> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerBuildCount>> {
        const STATEMENT: Statement =
            Statement::select_playerdata("SELECT name, uuid, build_count From playerdata");

        let query = sqlx::query::<MySql>(STATEMENT.sql).try_map(|row| {
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
        self.fetch_all_with_timeout(&STATEMENT, query).await
    }
}

#[async_trait]
impl VecDataSource<PlayerPlayTicks> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerPlayTicks>> {
        const STATEMENT: Statement =
            Statement::select_playerdata("SELECT name, uuid, playtick From playerdata");

        let query = sqlx::query::<MySql>(STATEMENT.sql).try_map(|row| {
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
        self.fetch_all_with_timeout(&STATEMENT, query).await
    }
}

#[async_trait]
impl VecDataSource<PlayerVoteCount> for MySqlDataSource {
    async fn fetch(&self) -> anyhow::Result<Vec<PlayerVoteCount>> {
        const STATEMENT: Statement = Statement {
            sql: "SELECT playerdata.name, playerdata.uuid, vote_number From vote INNER JOIN playerdata ON vote.uuid = playerdata.uuid",
            summary: "SELECT vote playerdata",
            collection: None,
        };

        let query = sqlx::query::<MySql>(STATEMENT.sql).try_map(|row| {
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
        });
        self.fetch_all_with_timeout(&STATEMENT, query).await
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
        database_name: config.database_name.clone(),
        server_address: config.host.clone(),
        server_port: config.port.0,
        metrics: DbOperationMetrics::new(&config.database_name),
    })
}
