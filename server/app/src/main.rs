#![deny(clippy::all)]
#![warn(clippy::nursery, clippy::pedantic, clippy::cargo)]
#![allow(clippy::cargo_common_metadata)]

use config::{AppConfig, FromEnv, SourceDatabaseConfig};
use infra_grpc::buf_generated::gigantic_minecraft::seichi_game_data::v1::read_service_server::{
    ReadService, ReadServiceServer,
};
use infra_grpc::read_service::ReadServiceImpl;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // OpenTelemetry pipeline: traces + metrics + logs over OTLP, plus JSON
    // logs to stdout (picked up by the cluster's log scraper). Configuration
    // is driven by `OTEL_*` environment variables (OTEL_SERVICE_NAME,
    // OTEL_EXPORTER_OTLP_ENDPOINT, OTEL_EXPORTER_OTLP_PROTOCOL, ...). Without
    // those vars only the stdout sink is active, so local runs do not require
    // an OTLP collector.
    let _otel_guard = init_tracing_opentelemetry::TracingConfig::default()
        .with_json_format()
        .with_stdout()
        .with_log_directives(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .with_otel(true)
        .with_metrics(true)
        .init_subscriber()?;

    let _profiler_guard = profiler::try_start()?;

    tracing::info!("Reading config...");
    let config = AppConfig::from_env()?;

    let service = initialize_database_read_service(&config.source_database_config)
        .await
        .expect("Initializing read service");

    let serve_address = format!("{}:{}", config.http_config.host, config.http_config.port.0)
        .parse()
        .expect("Parsing serve address from config");

    tracing::info!(%serve_address, "Server will be listening");

    Server::builder()
        .layer(OtelGrpcLayer::default().filter(filters::reject_healthcheck))
        .add_service(ReadServiceServer::new(service))
        .serve(serve_address)
        .await?;

    Ok(())
}
