//! `xxmi-mcp`: the hosted MCP server. Configuration is environment-only.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use mimalloc::MiMalloc;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};
use xxmi_mcp::app::{AppState, router};
use xxmi_mcp::catalog::Catalog;
use xxmi_mcp::config::Config;
use xxmi_mcp::exec::GitCommand;
use xxmi_mcp::indexer::{IndexerOptions, PollingIndexer};
use xxmi_mcp::logger::Logger;
use xxmi_mcp::packages::PackageCache;
use xxmi_mcp::selection::SelectionLimits;
use xxmi_mcp::spec_build::{SpecBuilder, builder_id};
use xxmi_mcp::spec_store::VolumeSpecs;
use xxmi_mcp::specs::SpecRegistry;
use xxmi_mcp::store::Store;
use xxmi_mcp::tools::ToolContext;
use xxmi_mcp::upstream::GitSources;
use xxmi_mcp::upstream::releases::GithubReleases;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

/// Tool calls park on blocking threads; a small pool keeps their memory bounded.
const MAX_BLOCKING_THREADS: usize = 8;

fn startup_failed(error: &str) -> ExitCode {
    let line = json!({ "level": "error", "msg": "startup failed", "error": error });
    eprintln!("{line}");
    ExitCode::FAILURE
}

async fn shutdown_signal(logger: Logger) {
    let (Ok(mut term), Ok(mut int)) = (
        signal(SignalKind::terminate()),
        signal(SignalKind::interrupt()),
    ) else {
        return std::future::pending().await;
    };
    let name = tokio::select! {
        _ = term.recv() => "SIGTERM",
        _ = int.recv() => "SIGINT",
    };
    logger.info("shutting down", json!({ "signal": name }));
}

fn polling_indexer(
    config: &Config,
    store: Store,
    catalog: Arc<Catalog>,
    specs: Arc<VolumeSpecs>,
    logger: Logger,
) -> Arc<PollingIndexer<GithubReleases>> {
    let options = IndexerOptions {
        sources: GitSources::new(store.data_dir.clone(), GitCommand::default(), logger),
        store,
        catalog,
        logger,
        limits: SelectionLimits {
            releases: config.indexed_releases as usize,
            prereleases: config.indexed_prereleases as usize,
        },
        git_cache_max_mb: config.git_cache_max_mb,
        poll_interval_min: config.poll_interval_min,
        specs,
        spec_builder: SpecBuilder::new(),
    };
    let releases = GithubReleases {
        token: config.github_token.clone(),
    };
    Arc::new(PollingIndexer::new(options, releases))
}

async fn serve(config: Config) -> std::io::Result<()> {
    let logger = Logger::default();
    let store = Store::new(&config.data_dir);
    store.init().await?;
    let catalog = Arc::new(Catalog::load(store.clone()).await);
    let ready = catalog.ready();
    let (port, host, indexer_enabled) = (config.port, config.host.clone(), config.indexer_enabled);

    let specs = Arc::new(VolumeSpecs::new(
        store.clone(),
        catalog.clone(),
        builder_id(),
    ));
    let indexer = polling_indexer(&config, store, catalog.clone(), specs.clone(), logger);
    let ctx = ToolContext {
        config,
        catalog,
        logger,
        specs: SpecRegistry::new(Box::new(specs)),
        packages: PackageCache::default(),
    };
    let app = router(AppState::new(ctx, indexer.clone()));

    let listener = TcpListener::bind((host.as_str(), port)).await?;
    let bound = listener.local_addr()?.port();
    logger.info(
        "listening",
        json!({ "port": bound, "ready": ready, "indexer": indexer_enabled }),
    );

    if indexer_enabled {
        indexer.start();
    }

    let service = app.into_make_service_with_connect_info::<SocketAddr>();
    axum::serve(listener, service)
        .with_graceful_shutdown(shutdown_signal(logger))
        .await
}

fn main() -> ExitCode {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => return startup_failed(&error),
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(config.worker_threads)
        .max_blocking_threads(MAX_BLOCKING_THREADS)
        .enable_all()
        .build();
    let runtime = match runtime {
        Ok(runtime) => runtime,
        Err(error) => return startup_failed(&error.to_string()),
    };

    match runtime.block_on(serve(config)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => startup_failed(&error.to_string()),
    }
}
