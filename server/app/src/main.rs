#![deny(clippy::all)]
#![warn(clippy::nursery, clippy::pedantic, clippy::cargo)]
#![allow(clippy::cargo_common_metadata)]

use anyhow::Context;
use config::{AppConfig, FromEnv, SourceDatabaseConfig};
use infra_grpc::buf_generated::gigantic_minecraft::seichi_game_data::v1::read_service_server::{
    ReadService, ReadServiceServer,
};
use infra_grpc::read_service::ReadServiceImpl;
use std::time::Duration;
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::sync::watch;
use tonic::transport::Server;
use tonic_tracing_opentelemetry::middleware::{filters, server::OtelGrpcLayer};

// Continuous profiling agent (pyroscope-rs).
//
// Pushes pprof CPU profiles to a Pyroscope server while the returned guard is
// alive. Disabled at runtime when `PYROSCOPE_SERVER_ADDRESS` is not set so the
// binary remains usable in environments without a profiler endpoint.
mod profiler {
    use pyroscope::backend::{BackendConfig, PprofConfig, pprof_backend};
    use pyroscope::pyroscope::PyroscopeAgentBuilder;

    pub struct Guard {
        shutdown: Option<Box<dyn FnOnce() + Send>>,
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            if let Some(f) = self.shutdown.take() {
                f();
            }
        }
    }

    pub fn try_start() -> anyhow::Result<Guard> {
        let Ok(server_address) = std::env::var("PYROSCOPE_SERVER_ADDRESS") else {
            tracing::info!("PYROSCOPE_SERVER_ADDRESS unset; continuous profiler disabled");
            return Ok(Guard { shutdown: None });
        };

        let app_name = std::env::var("PYROSCOPE_APPLICATION_NAME")
            .unwrap_or_else(|_| env!("CARGO_PKG_NAME").to_string());
        let sample_rate: u32 = std::env::var("PYROSCOPE_SAMPLE_RATE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100);

        let agent = PyroscopeAgentBuilder::new(
            &server_address,
            &app_name,
            sample_rate,
            "pyroscope-rs",
            env!("CARGO_PKG_VERSION"),
            pprof_backend(PprofConfig::default(), BackendConfig::default()),
        )
        .build()?;
        let running = agent.start()?;
        tracing::info!(
            server = %server_address,
            application = %app_name,
            sample_rate = sample_rate,
            "continuous profiler started"
        );

        Ok(Guard {
            shutdown: Some(Box::new(move || match running.stop() {
                Ok(ready) => ready.shutdown(),
                Err(e) => eprintln!("pyroscope agent stop failed: {e}"),
            })),
        })
    }
}

async fn initialize_database_read_service(
    config: &SourceDatabaseConfig,
) -> anyhow::Result<impl ReadService> {
    use infra_repository_impl::mysql_data_source;

    let data_source = Box::new(mysql_data_source::from_config(config).await?);

    Ok(ReadServiceImpl {
        last_quit_data_source: data_source.clone(),
        break_counts_data_source: data_source.clone(),
        build_counts_data_source: data_source.clone(),
        play_ticks_data_source: data_source.clone(),
        vote_counts_data_source: data_source,
    })
}

/// How long in-flight RPCs may keep running after a shutdown signal before
/// the server is dropped. Kept well below the pod's
/// `terminationGracePeriodSeconds` (30s) so that the telemetry flush, and in
/// particular the pyroscope agent, which waits for its next 10s report cycle
/// when stopped, still finishes before SIGKILL.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(15);

/// Upper bound on how long a single RPC may take on the server side.
///
/// A safety net on top of the per-query timeout (`DB_QUERY_TIMEOUT_SECS`,
/// default 60s) plus the 5s pool acquire timeout, so it must stay longer than
/// their sum. It is also shorter than the longest client-side timeout
/// (conifers, 120s), so a stuck RPC is cut by the server and reported as an
/// error instead of being left running after the caller gave up. tonic also
/// honours a shorter `grpc-timeout` header sent by the client.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

/// Interval and ack timeout of HTTP/2 PING frames sent to clients, so that
/// connections whose peer silently disappeared (e.g. a pod that was killed
/// without closing its sockets) are detected and closed.
const HTTP2_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(60);
const HTTP2_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(20);

/// TCP keepalive on accepted sockets, as a lower-level backstop for the same
/// half-open connection problem.
const TCP_KEEPALIVE: Duration = Duration::from_secs(60);

/// Resolves when the process receives SIGTERM (sent by the kubelet on pod
/// termination) or SIGINT (Ctrl-C in local runs).
async fn shutdown_signal(mut sigterm: Signal) {
    let signal_name = tokio::select! {
        _ = sigterm.recv() => "SIGTERM",
        result = tokio::signal::ctrl_c() => {
            if let Err(e) = result {
                tracing::error!(error = %e, "failed to listen for SIGINT");
            }
            "SIGINT"
        }
    };

    tracing::info!(
        signal = signal_name,
        "shutdown signal received; draining in-flight requests"
    );
}

/// Resolves once `shutdown_signal` has fired.
async fn shutdown_requested(mut rx: watch::Receiver<bool>) {
    // The sender is only dropped after it has sent `true`.
    let _ = rx.wait_for(|requested| *requested).await;
}

async fn run(shutdown: watch::Receiver<bool>) -> anyhow::Result<()> {
    tracing::info!("Reading config...");
    let config = AppConfig::from_env()?;

    // Connecting to MySQL can block for a while; stop right away if a
    // shutdown is requested in the meantime.
    let service = tokio::select! {
        service = initialize_database_read_service(&config.source_database_config) => {
            service.context("Initializing read service")?
        }
        () = shutdown_requested(shutdown.clone()) => return Ok(()),
    };

    let serve_address = format!("{}:{}", config.http_config.host, config.http_config.port.0)
        .parse()
        .context("Parsing serve address from config")?;

    tracing::info!(%serve_address, "Server will be listening");

    let serve = Server::builder()
        .timeout(REQUEST_TIMEOUT)
        .http2_keepalive_interval(Some(HTTP2_KEEPALIVE_INTERVAL))
        .http2_keepalive_timeout(Some(HTTP2_KEEPALIVE_TIMEOUT))
        .tcp_keepalive(Some(TCP_KEEPALIVE))
        .layer(OtelGrpcLayer::default().filter(filters::reject_healthcheck))
        .add_service(ReadServiceServer::new(service))
        .serve_with_shutdown(serve_address, shutdown_requested(shutdown.clone()));
    tokio::pin!(serve);

    tokio::select! {
        biased;
        result = &mut serve => result?,
        () = shutdown_requested(shutdown) => {
            if let Ok(result) = tokio::time::timeout(DRAIN_TIMEOUT, &mut serve).await {
                result?;
            } else {
                tracing::warn!(
                    timeout = ?DRAIN_TIMEOUT,
                    "in-flight requests did not finish in time; abandoning them"
                );
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // OpenTelemetry pipeline: traces + metrics + logs over OTLP, plus JSON
    // logs to stdout (picked up by the cluster's log scraper). Configuration
    // is driven by `OTEL_*` environment variables (OTEL_SERVICE_NAME,
    // OTEL_EXPORTER_OTLP_ENDPOINT, OTEL_EXPORTER_OTLP_PROTOCOL, ...). Without
    // those vars only the stdout sink is active, so local runs do not require
    // an OTLP collector.
    let otel_guard = init_tracing_opentelemetry::TracingConfig::default()
        .with_json_format()
        .with_stdout()
        .with_log_directives(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .with_otel(true)
        .with_metrics(true)
        .init_subscriber()?;

    let profiler_guard = profiler::try_start()?;

    // The binary runs as PID 1 in a distroless image, where the kernel ignores
    // signals that have no handler, so SIGTERM must be handled here for the
    // pod to stop before `terminationGracePeriodSeconds` elapses. Install the
    // handler before anything slow (such as connecting to MySQL) happens.
    let sigterm = signal(SignalKind::terminate())?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal(sigterm).await;
        let _ = shutdown_tx.send(true);
    });

    let result = run(shutdown_rx).await;
    if let Err(e) = &result {
        tracing::error!(error = ?e, "server exited with an error");
    }

    tracing::info!("shutdown complete; flushing telemetry");

    // Flush the OpenTelemetry pipeline first so the logs above reach the
    // collector, then stop the profiler (which uploads its last profile).
    // Done explicitly so the order also holds when `run` fails.
    drop(otel_guard);
    drop(profiler_guard);

    Ok(result?)
}
