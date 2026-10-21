//! gRPC サーバー側のメトリクス。
//!
//! 各 RPC の所要時間を `rpc.server.call.duration` (単位は秒) に記録する
//! tower layer。
//!
//! メトリクス名と単位は現行の semantic conventions に合わせた (旧名の
//! `rpc.server.duration` はミリ秒単位で deprecated)。一方で属性は、同じ
//! リクエストの server span (`tonic-tracing-opentelemetry` が作る) と同じ旧来の
//! `rpc.system` / `rpc.service` / `rpc.method` / `rpc.grpc.status_code` にして
//! いる。現行の conventions (`rpc.system.name` / `rpc.response.status_code` 等)
//! はまだ release candidate で、トレースと同じラベルで突き合わせられることを
//! 優先した。
//!
//! 計測はレスポンスボディを送り終えるまで (trailer の `grpc-status` を読むまで)
//! 行う。ハンドラが返った時点ではまだ protobuf のエンコードと送信が終わって
//! おらず、全プレイヤー分を返すこのサービスではその時間が無視できないため。
//!
//! メーターは global meter provider から取るので、[`RpcMetricsLayer::new`] は
//! tracing / metrics の初期化より後に呼ぶこと。

use http_body::{Body, Frame, SizeHint};
use opentelemetry::KeyValue;
use opentelemetry::metrics::Histogram;
use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;
use tonic::codegen::http::{self, HeaderMap};
use tower::{Layer, Service};

/// 秒単位のヒストグラムのバケット境界。
///
/// semantic conventions が推奨する境界 (5ms から 10s) に、クエリタイムアウト
/// (既定 60s) とリクエストタイムアウト (`REQUEST_TIMEOUT`、90s) までを見られる
/// よう 30s / 60s / 90s を足したもの。SDK の既定の境界はミリ秒を想定している
/// ので、秒で記録する場合は明示する。
const DURATION_BUCKETS_SECONDS: [f64; 17] = [
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0, 30.0, 60.0,
    90.0,
];

/// `grpc-status` の値。
const GRPC_OK: i64 = 0;
const GRPC_CANCELLED: i64 = 1;
const GRPC_UNKNOWN: i64 = 2;
const GRPC_UNIMPLEMENTED: i64 = 12;

/// 属性に使えないサービス名・メソッド名の代わりに入れる値。
const OTHER: &str = "_OTHER";

#[derive(Debug, Clone)]
pub struct RpcMetricsLayer {
    duration: Histogram<f64>,
    known_services: &'static [&'static str],
}

impl RpcMetricsLayer {
    /// `known_services` はこのサーバーが提供する gRPC サービスの完全修飾名。
    /// それ以外のパスへのリクエストは `rpc.service` / `rpc.method` を
    /// `_OTHER` にまとめ、任意のパスでラベルの種類が増えないようにする。
    pub fn new(known_services: &'static [&'static str]) -> Self {
        let duration = opentelemetry::global::meter(env!("CARGO_PKG_NAME"))
            .f64_histogram("rpc.server.call.duration")
            .with_unit("s")
            .with_description("Measures the duration of inbound RPC.")
            .with_boundaries(DURATION_BUCKETS_SECONDS.to_vec())
            .build();
        Self {
            duration,
            known_services,
        }
    }
}

impl<S> Layer<S> for RpcMetricsLayer {
    type Service = RpcMetricsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RpcMetricsService {
            inner,
            layer: self.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RpcMetricsService<S> {
    inner: S,
    layer: RpcMetricsLayer,
}

impl<S, ReqBody, ResBody> Service<http::Request<ReqBody>> for RpcMetricsService<S>
where
    S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>>,
    ResBody: Body,
{
    type Response = http::Response<MetricsBody<ResBody>>;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        let recorder = Recorder::start(&self.layer, req.uri().path());
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
    ResBody: Body,
{
    type Output = Result<http::Response<MetricsBody<ResBody>>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result = std::task::ready!(this.inner.poll(cx));
        let mut recorder = this.recorder.take();
        Poll::Ready(match result {
            Ok(response) => {
                if let Some(status) = grpc_status_in(response.headers()) {
                    // trailers-only レスポンス (エラー)。ボディは空なのでここで終わり。
                    if let Some(recorder) = recorder.as_mut() {
                        recorder.finish(status);
                    }
                }
                let fallback_status = status_from_http(response.status());
                Ok(response.map(|inner| MetricsBody {
                    inner,
                    recorder,
                    fallback_status,
                }))
            }
            Err(e) => {
                // tonic はハンドラのエラーも `grpc-status` 付きの HTTP レスポンス
                // として返すので、ここに来るのは transport レベルの失敗だけ。
                if let Some(recorder) = recorder.as_mut() {
                    recorder.finish(GRPC_UNKNOWN);
                }
                Err(e)
            }
        })
    }
}

pin_project! {
    /// レスポンスボディを包み、送り終えた時点で所要時間を記録する。
    pub struct MetricsBody<B>
    where
        B: Body,
    {
        #[pin]
        inner: B,
        recorder: Option<Recorder>,
        // trailer に `grpc-status` が無いまま終わった場合に使うステータス。
        fallback_status: i64,
    }

    impl<B> PinnedDrop for MetricsBody<B>
    where
        B: Body,
    {
        fn drop(this: Pin<&mut Self>) {
            let this = this.project();
            if let Some(recorder) = this.recorder.as_mut() {
                // 最後まで読まれたがボディの終わり (`None`) を poll されずに
                // drop された場合は正常に終わったものとして扱う。そうでなければ
                // 送信の途中でストリームがリセットされたので、Recorder の
                // Drop が CANCELLED として記録する。
                if this.inner.as_ref().get_ref().is_end_stream() {
                    recorder.finish(*this.fallback_status);
                }
            }
        }
    }
}

impl<B: Body> Body for MetricsBody<B> {
    type Data = B::Data;
    type Error = B::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.project();
        let frame = std::task::ready!(this.inner.poll_frame(cx));
        if let Some(recorder) = this.recorder.as_mut() {
            match &frame {
                // trailer は最後のフレームなので、ここで記録する (hyper は
                // trailer を送った後にボディを poll しないことがある)。
                Some(Ok(frame)) => {
                    if let Some(trailers) = frame.trailers_ref() {
                        recorder.finish(grpc_status_in(trailers).unwrap_or(*this.fallback_status));
                    }
                }
                Some(Err(_)) => recorder.finish(GRPC_UNKNOWN),
                None => recorder.finish(*this.fallback_status),
            }
        }
        Poll::Ready(frame)
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

/// 1 回の RPC の計測。
///
/// [`Recorder::finish`] を呼ばずに drop された場合は CANCELLED として記録する。
/// レスポンスを返す前に future ごと drop されるのは、クライアントの切断か、
/// tonic の `Server::timeout` / `grpc-timeout` による打ち切り (この layer より
/// 外側にある) のどちらかで、tonic はタイムアウトもクライアントに CANCELLED
/// (`Timeout expired`) として返すので、どちらの場合も呼び出し元が見る
/// ステータスと一致する。
struct Recorder {
    duration: Histogram<f64>,
    start: Instant,
    service: &'static str,
    method: String,
    finished: bool,
}

impl Recorder {
    /// ヘルスチェックは計測しない (トレースの `filters::reject_healthcheck` と揃える)。
    fn start(layer: &RpcMetricsLayer, path: &str) -> Option<Self> {
        if path.starts_with("/grpc.health.v1.Health/") {
            return None;
        }
        // gRPC のパスは `/{package}.{Service}/{Method}`。
        let known = path
            .strip_prefix('/')
            .and_then(|p| p.split_once('/'))
            .and_then(|(service, method)| {
                let service = layer.known_services.iter().find(|s| **s == service)?;
                (!method.is_empty() && !method.contains('/')).then_some((*service, method))
            });
        let (service, method) = known.unwrap_or((OTHER, OTHER));
        Some(Self {
            duration: layer.duration.clone(),
            start: Instant::now(),
            service,
            method: method.to_owned(),
            finished: false,
        })
    }

    fn finish(&mut self, status: i64) {
        if self.finished {
            return;
        }
        self.finished = true;

        // 存在しないメソッドへの呼び出し (UNIMPLEMENTED) はメソッド名が任意の
        // 値になりうるのでまとめる。
        // 記録は 1 回だけなので、メソッド名は複製せずに取り出す。
        let (service, method) = if status == GRPC_UNIMPLEMENTED {
            (OTHER, OTHER.to_owned())
        } else {
            (self.service, std::mem::take(&mut self.method))
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
        self.finish(GRPC_CANCELLED);
    }
}

fn grpc_status_in(headers: &HeaderMap) -> Option<i64> {
    headers
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
}

/// `grpc-status` が無いときに HTTP ステータスから推定する。
///
/// <https://github.com/grpc/grpc/blob/master/doc/http-grpc-status-mapping.md>
fn status_from_http(status: http::StatusCode) -> i64 {
    match status {
        http::StatusCode::OK => GRPC_OK,
        http::StatusCode::BAD_REQUEST => 13,
        http::StatusCode::UNAUTHORIZED => 16,
        http::StatusCode::FORBIDDEN => 7,
        http::StatusCode::NOT_FOUND => GRPC_UNIMPLEMENTED,
        http::StatusCode::TOO_MANY_REQUESTS
        | http::StatusCode::BAD_GATEWAY
        | http::StatusCode::SERVICE_UNAVAILABLE
        | http::StatusCode::GATEWAY_TIMEOUT => 14,
        _ => GRPC_UNKNOWN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVICE: &str = "gigantic_minecraft.seichi_game_data.v1.ReadService";

    fn layer() -> RpcMetricsLayer {
        RpcMetricsLayer::new(&[SERVICE])
    }

    #[test]
    fn grpc_status_from_header() {
        let mut headers = HeaderMap::new();
        headers.insert("grpc-status", "4".parse().unwrap());
        assert_eq!(grpc_status_in(&headers), Some(4));
        assert_eq!(grpc_status_in(&HeaderMap::new()), None);
    }

    #[test]
    fn grpc_status_from_http_status() {
        assert_eq!(status_from_http(http::StatusCode::OK), GRPC_OK);
        assert_eq!(status_from_http(http::StatusCode::SERVICE_UNAVAILABLE), 14);
    }

    #[test]
    fn healthcheck_is_not_recorded() {
        assert!(Recorder::start(&layer(), "/grpc.health.v1.Health/Check").is_none());
    }

    #[test]
    fn known_service_and_method_are_split_from_path() {
        let recorder = Recorder::start(&layer(), &format!("/{SERVICE}/LastQuits")).unwrap();
        assert_eq!(recorder.service, SERVICE);
        assert_eq!(recorder.method, "LastQuits");
    }

    #[test]
    fn unknown_service_is_folded() {
        let recorder = Recorder::start(&layer(), "/x.Random1234/Method").unwrap();
        assert_eq!(recorder.service, OTHER);
        assert_eq!(recorder.method, OTHER);
        let recorder = Recorder::start(&layer(), "/not-grpc").unwrap();
        assert_eq!(recorder.service, OTHER);
    }
}
