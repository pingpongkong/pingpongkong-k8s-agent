mod configs;
mod controllers;
mod errors;
mod infra;
mod models;
mod schedulers;
mod services;

use configs::AppConfig;
use controllers::{AppState, router};
use kube::Client;
use models::{SharedCache, SharedTasks};
use schedulers::run_probe_loop;
use services::run_config_watcher;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

/// Starts the agent background workers and local HTTP server.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::from_env()?;
    init_logging(config.log_level);
    info!(
        log_level = ?config.log_level,
        node_name = %config.config_map.node_name,
        config_namespace = %config.config_map.namespace,
        config_map = %config.config_map.name,
        config_key = %config.config_map.key,
        agent_api_port = config.agent_api_port,
        agent_check_interval = %format_duration(config.agent_check_interval),
        "PingPongKong agent starting"
    );

    let cache: SharedCache = Arc::new(RwLock::new(HashMap::new()));
    let tasks: SharedTasks = Arc::new(RwLock::new(Vec::new()));

    let prober_cache = Arc::clone(&cache);
    let prober_tasks = Arc::clone(&tasks);
    let check_interval = config.agent_check_interval;

    tokio::spawn(async move {
        run_probe_loop(prober_tasks, prober_cache, check_interval).await;
    });

    let watcher_options = config.config_map.clone();
    let watcher_cache = Arc::clone(&cache);
    let watcher_tasks = Arc::clone(&tasks);
    tokio::spawn(async move {
        if let Err(err) = run_config_watcher(watcher_options, watcher_tasks, watcher_cache).await {
            error!(error = %err, "config watcher stopped");
        }
    });

    let kube_client = Client::try_default().await?;
    let app = router(AppState::new(
        cache,
        tasks,
        config.config_map.node_name,
        kube_client,
    ));

    let bind_address = format!("0.0.0.0:{}", config.agent_api_port);
    let listener = tokio::net::TcpListener::bind(&bind_address).await?;
    info!(
        bind_address = %bind_address,
        port = config.agent_api_port,
        endpoints = "/, /status, /metrics, /state",
        "agent HTTP API listening"
    );

    axum::serve(listener, app).await?;

    Ok(())
}

fn init_logging(log_level: configs::LogLevel) {
    let filter = EnvFilter::builder()
        .with_default_directive(tracing::Level::from(log_level).into())
        .from_env_lossy();
    tracing_subscriber::fmt()
        .compact()
        .with_target(false)
        .with_env_filter(filter)
        .init();
}

fn format_duration(duration: std::time::Duration) -> String {
    let seconds = duration.as_secs();

    if seconds == 0 {
        format!("{}ms", duration.as_millis())
    } else if seconds % 3600 == 0 {
        format!("{}h", seconds / 3600)
    } else if seconds % 60 == 0 {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}
