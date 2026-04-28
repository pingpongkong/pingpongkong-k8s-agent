mod config;
mod kubernetes;
mod probe;
mod server;

use kubernetes::{ConfigMapWatchOptions, run_config_watcher};
use probe::{SharedCache, SharedTasks, run_probe_loop};
use server::{AppState, router};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Starts the agent background workers and local HTTP server.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("PingPongKong Agent starting...");

    let cache: SharedCache = Arc::new(RwLock::new(HashMap::new()));
    let tasks: SharedTasks = Arc::new(RwLock::new(Vec::new()));

    let prober_cache = Arc::clone(&cache);
    let prober_tasks = Arc::clone(&tasks);

    tokio::spawn(async move {
        run_probe_loop(prober_tasks, prober_cache).await;
    });

    let watcher_options = ConfigMapWatchOptions::from_env()?;
    let watcher_cache = Arc::clone(&cache);
    let watcher_tasks = Arc::clone(&tasks);
    tokio::spawn(async move {
        if let Err(err) = run_config_watcher(watcher_options, watcher_tasks, watcher_cache).await {
            eprintln!("config watcher stopped: {err:#}");
        }
    });

    let app = router(AppState::new(cache, tasks));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    println!("Axum server listening on :8080/metrics and :8080/state");

    axum::serve(listener, app).await?;

    Ok(())
}
