//! gRPC サーバー側のメトリクス。
//!
//! 各 RPC の所要時間を `rpc.server.call.duration` (単位は秒) に記録する
//! tower layer。名前と単位は現行の semantic conventions に従う (旧名の
//! `rpc.server.duration` はミリ秒単位で deprecated)。属性は同じリクエストの
//! server span (`tonic-tracing-opentelemetry` が作る) と同じ `rpc.system` /
//! `rpc.service` / `rpc.method` / `rpc.grpc.status_code` にして、メトリクスと
//! トレースを同じラベルで突き合わせられるようにしている。
//!
//! メーターは global meter provider から取るので、[`RpcMetricsLayer::new`] は
//! tracing / metrics の初期化より後に呼ぶこと。

use opentelemetry::KeyValue;
use opentelemetry::metrics::Histogram;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;
use tonic::codegen::http;
use tower::{Layer, Service};

/// 秒単位のヒストグラムのバケット境界。
///
/// semantic conventions が推奨する境界 (5ms から 10s) に、クエリタイムアウト
/// (既定 60s) とリクエストタイムアウト (`REQUEST_TIMEOUT`、90s) までを見られる
/// よう 30s / 60s / 90s を足したもの。
const DURATION_BUCKETS_SECONDS: [f64; 17] = [
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0, 30.0, 60.0,
    90.0,
];

/// `grpc-status` の `CANCELLED`。
const GRPC_CANCELLED: i64 = 1;
/// `grpc-status` の `UNIMPLEMENTED`。
const GRPC_UNIMPLEMENTED: i64 = 12;

#[derive(Debug, Clone)]
pub struct RpcMetricsLayer {
    duration: Histogram<f64>,
}

impl RpcMetricsLayer {
    pub fn new() -> Self {
        let duration = opentelemetry::global::meter(env!("CARGO_PKG_NAME"))
            .f64_histogram("rpc.server.call.duration")
            .with_unit("s")
            .with_description("Measures the duration of inbound RPC.")
            .with_boundaries(DURATION_BUCKETS_SECONDS.to_vec())
            .build();
        Self { duration }
    }
}

impl<S> Layer<S> for RpcMetricsLayer {
    type Service = RpcMetricsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RpcMetricsService {
            inner,
            duration: self.duration.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RpcMetricsService<S> {
    inner: S,
    duration: Histogram<f64>,
}

impl<S, ReqBody, ResBody> Service<http::Request<ReqBody>> for RpcMetricsService<S>
where
    S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        let recorder = Recorder::start(self.duration.clone(), req.uri().path());
        ResponseFuture {
            inner: self.inner.call(req),
            recorder,
        }
    }
}

pin_project! {
    pub struct ResponseFuture<F> {
        #[pin]
        inner: F,
        recorder: Option<Recorder>,
    }
}

impl<F, ResBody, E> Future for ResponseFuture<F>
where
    F: Future<Output = Result<http::Response<ResBody>, E>>,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result = std::task::ready!(this.inner.poll(cx));
        if let Some(recorder) = this.recorder.take() {
            // tonic はハンドラのエラーも `grpc-status` 付きの HTTP レスポンス
            // として返すので、`Err` になるのは transport レベルの失敗だけ。
            let status = result.as_ref().map_or(2, grpc_status_of);
            recorder.finish(status);
        }
        Poll::Ready(result)
    }
}

/// 1 回の RPC の計測。完了前に drop された場合も記録する。
struct Recorder {
    duration: Histogram<f64>,
    start: Instant,
    service: String,
    method: String,
}

impl Recorder {
    /// ヘルスチェックは計測しない (トレースの `filters::reject_healthcheck` と揃える)。
    fn start(duration: Histogram<f64>, path: &str) -> Option<Self> {
        if path.starts_with("/grpc.health.v1.Health/") {
            return None;
        }
        // gRPC のパスは `/{package}.{Service}/{Method}`。
        let (service, method) = path
            .strip_prefix('/')
            .and_then(|p| p.split_once('/'))
            .filter(|(service, method)| !service.is_empty() && !method.is_empty())
            .unwrap_or(("_OTHER", "_OTHER"));
        Some(Self {
            duration,
            start: Instant::now(),
            service: service.to_owned(),
            method: method.to_owned(),
        })
    }

    fn finish(mut self, status: i64) {
        self.record(status);
        // `Drop` で二重に記録しないよう、記録済みの印として空にする。
        self.service.clear();
    }

    fn record(&self, status: i64) {
        // 存在しないメソッドへの呼び出し (UNIMPLEMENTED) はパスをそのまま
        // 属性にするとカーディナリティが際限なく増えるので丸める。
        let (service, method) = if status == GRPC_UNIMPLEMENTED {
            ("_OTHER".to_owned(), "_OTHER".to_owned())
        } else {
            (self.service.clone(), self.method.clone())
        };
        self.duration.record(
            self.start.elapsed().as_secs_f64(),
            &[
                KeyValue::new("rpc.system", "grpc"),
                KeyValue::new("rpc.service", service),
                KeyValue::new("rpc.method", method),
                KeyValue::new("rpc.grpc.status_code", status),
            ],
        );
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if !self.service.is_empty() {
            // レスポンスを返す前に future が drop された。クライアントの切断に
            // 加え、tonic の `Server::timeout` / `grpc-timeout` による打ち切りも
            // ここに来る (タイムアウトの layer はこの layer より外側にあり、
            // クライアントには DEADLINE_EXCEEDED / CANCELLED が返るが、
            // ここからはどちらか区別できない)。サーバー側から見ると処理が
            // 取り消されたので CANCELLED として記録する。
            self.record(GRPC_CANCELLED);
        }
    }
}

/// レスポンスの `grpc-status` を返す。
///
/// 成功したレスポンスでは `grpc-status` はヘッダーではなく trailer に入るので、
/// ヘッダーに無く HTTP ステータスが 200 なら OK (0) とみなす。エラーは
/// trailers-only レスポンスとしてヘッダーに入る。
fn grpc_status_of<B>(response: &http::Response<B>) -> i64 {
    if let Some(status) = response
        .headers()
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
    {
        return status;
    }
    // https://github.com/grpc/grpc/blob/master/doc/http-grpc-status-mapping.md
    match response.status() {
        http::StatusCode::OK => 0,
        http::StatusCode::BAD_REQUEST => 13,
        http::StatusCode::UNAUTHORIZED => 16,
        http::StatusCode::FORBIDDEN => 7,
        http::StatusCode::NOT_FOUND => GRPC_UNIMPLEMENTED,
        http::StatusCode::TOO_MANY_REQUESTS
        | http::StatusCode::BAD_GATEWAY
        | http::StatusCode::SERVICE_UNAVAILABLE
        | http::StatusCode::GATEWAY_TIMEOUT => 14,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: http::StatusCode, grpc_status: Option<&str>) -> http::Response<()> {
        let mut builder = http::Response::builder().status(status);
        if let Some(grpc_status) = grpc_status {
            builder = builder.header("grpc-status", grpc_status);
        }
        builder.body(()).unwrap()
    }

    #[test]
    fn grpc_status_from_header() {
        assert_eq!(
            grpc_status_of(&response(http::StatusCode::OK, Some("4"))),
            4
        );
    }

    #[test]
    fn grpc_status_ok_without_header() {
        assert_eq!(grpc_status_of(&response(http::StatusCode::OK, None)), 0);
    }

    #[test]
    fn grpc_status_from_http_status() {
        assert_eq!(
            grpc_status_of(&response(http::StatusCode::SERVICE_UNAVAILABLE, None)),
            14
        );
    }

    #[test]
    fn healthcheck_is_not_recorded() {
        let histogram = opentelemetry::global::meter("test")
            .f64_histogram("h")
            .build();
        assert!(Recorder::start(histogram, "/grpc.health.v1.Health/Check").is_none());
    }

    #[test]
    fn service_and_method_are_split_from_path() {
        let histogram = opentelemetry::global::meter("test")
            .f64_histogram("h")
            .build();
        let recorder = Recorder::start(
            histogram,
            "/gigantic_minecraft.seichi_game_data.v1.ReadService/LastQuits",
        )
        .unwrap();
        assert_eq!(
            recorder.service,
            "gigantic_minecraft.seichi_game_data.v1.ReadService"
        );
        assert_eq!(recorder.method, "LastQuits");
        recorder.finish(0);
    }
}
