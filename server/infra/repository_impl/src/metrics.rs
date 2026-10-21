//! DB クライアント側のメトリクス。
//!
//! OpenTelemetry semantic conventions の `db.client.operation.duration`
//! (単位は秒) を記録する。メーターはプロセス全体の global meter provider
//! (`init-tracing-opentelemetry` が OTLP exporter 付きで設定する) から取るので、
//! [`DbOperationMetrics::new`] は tracing / metrics の初期化より後に呼ぶこと。
//! それより前に作った instrument は何も記録しない no-op のままになる。

use opentelemetry::KeyValue;
use opentelemetry::metrics::Histogram;
use std::sync::Arc;
use std::time::Duration;

/// 秒単位のヒストグラムのバケット境界。
///
/// semantic conventions が推奨する境界 (5ms から 10s) に、このサービスの
/// クエリタイムアウト (既定 60s) までを見られるよう 30s / 60s を足したもの。
/// SDK の既定の境界はミリ秒を想定しているので、秒で記録する場合は明示する。
const DURATION_BUCKETS_SECONDS: [f64; 16] = [
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0, 30.0, 60.0,
];

#[derive(Debug, Clone)]
pub struct DbOperationMetrics {
    duration: Histogram<f64>,
    // 毎回の記録で文字列を複製しないよう Arc で持つ。
    namespace: Arc<str>,
}

impl DbOperationMetrics {
    pub fn new(namespace: &str) -> Self {
        let duration = opentelemetry::global::meter(env!("CARGO_PKG_NAME"))
            .f64_histogram("db.client.operation.duration")
            .with_unit("s")
            .with_description("Duration of database client operations.")
            .with_boundaries(DURATION_BUCKETS_SECONDS.to_vec())
            .build();

        Self {
            duration,
            namespace: Arc::from(namespace),
        }
    }

    /// 1 回の DB 操作 (接続の取得 + クエリの実行) の所要時間を記録する。
    ///
    /// `collection` は `db.collection.name` (JOIN など対象が 1 つに決まらない
    /// ときは `None`)、`error_type` は失敗したときの `error.type`。
    pub fn record(
        &self,
        elapsed: Duration,
        collection: Option<&'static str>,
        error_type: Option<&'static str>,
    ) {
        let mut attributes = vec![
            KeyValue::new("db.system.name", "mysql"),
            KeyValue::new("db.namespace", Arc::clone(&self.namespace)),
            KeyValue::new("db.operation.name", "SELECT"),
        ];
        if let Some(collection) = collection {
            attributes.push(KeyValue::new("db.collection.name", collection));
        }
        if let Some(error_type) = error_type {
            attributes.push(KeyValue::new("error.type", error_type));
        }

        self.duration.record(elapsed.as_secs_f64(), &attributes);
    }
}

/// `sqlx::Error` を `error.type` に使う低カーディナリティな文字列に変換する。
pub fn sqlx_error_type(error: &sqlx::Error) -> &'static str {
    match error {
        sqlx::Error::PoolTimedOut => "pool_timed_out",
        sqlx::Error::PoolClosed => "pool_closed",
        sqlx::Error::Io(_) => "io",
        sqlx::Error::Tls(_) => "tls",
        sqlx::Error::Protocol(_) => "protocol",
        sqlx::Error::Database(_) => "database",
        sqlx::Error::ColumnNotFound(_)
        | sqlx::Error::ColumnIndexOutOfBounds { .. }
        | sqlx::Error::ColumnDecode { .. }
        | sqlx::Error::Decode(_) => "decode",
        _ => "_OTHER",
    }
}
