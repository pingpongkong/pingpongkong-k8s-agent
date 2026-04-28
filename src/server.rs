use crate::probe::{ProbeResult, SharedCache, SharedTasks};
use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use std::collections::HashMap;

#[derive(Clone)]
pub struct AppState {
    cache: SharedCache,
    tasks: SharedTasks,
}

impl AppState {
    /// Creates the HTTP application state shared by all Axum handlers.
    pub fn new(cache: SharedCache, tasks: SharedTasks) -> Self {
        Self { cache, tasks }
    }
}

/// Builds the local HTTP router for Prometheus metrics and debug state.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/state", get(state_handler))
        .with_state(state)
}

/// Renders the latest probe state in Prometheus text exposition format.
async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let cache = state.cache.read().await;
    let active_tasks = state.tasks.read().await.len();
    let mut body = String::new();

    body.push_str("# HELP pingpongkong_active_probe_tasks Number of active probe tasks.\n");
    body.push_str("# TYPE pingpongkong_active_probe_tasks gauge\n");
    body.push_str(&format!("pingpongkong_active_probe_tasks {active_tasks}\n"));
    body.push_str(
        "# HELP pingpongkong_probe_success Last probe success, 1 for success and 0 for failure.\n",
    );
    body.push_str("# TYPE pingpongkong_probe_success gauge\n");
    body.push_str(
        "# HELP pingpongkong_probe_latency_milliseconds Last probe latency in milliseconds.\n",
    );
    body.push_str("# TYPE pingpongkong_probe_latency_milliseconds gauge\n");

    for result in cache.values() {
        let labels = format!(
            "protocol=\"{}\",source_role=\"{}\",target=\"{}\",target_name=\"{}\",port=\"{}\"",
            escape_label_value(&result.protocol),
            escape_label_value(&result.source_role),
            escape_label_value(&result.target),
            escape_label_value(&result.target_name),
            result.port,
        );

        body.push_str(&format!(
            "pingpongkong_probe_success{{{labels}}} {}\n",
            if result.success { 1 } else { 0 }
        ));
        body.push_str(&format!(
            "pingpongkong_probe_latency_milliseconds{{{labels}}} {}\n",
            result.latency_ms
        ));
    }

    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        body,
    )
}

/// Returns the latest probe state as JSON for local debugging.
async fn state_handler(State(state): State<AppState>) -> Json<HashMap<String, ProbeResult>> {
    let current_state = state.cache.read().await;
    Json(current_state.clone())
}

/// Escapes a value so it is safe inside a Prometheus label string.
fn escape_label_value(value: &str) -> String {
    value
        .replace('\\', r"\\")
        .replace('\n', r"\n")
        .replace('"', r#"\""#)
}
