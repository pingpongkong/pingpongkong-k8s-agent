use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type SharedCache = Arc<RwLock<HashMap<String, ProbeResult>>>;
pub type SharedTasks = Arc<RwLock<Vec<ProbeTask>>>;

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct ProbeTask {
    pub target: String,
    pub port: u16,
    pub protocol: String,
    pub source_role: String,
    pub target_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub success: bool,
    pub latency_ms: u128,
    pub last_probe_unix_seconds: u64,
    pub protocol: String,
    pub source_role: String,
    pub target: String,
    pub target_name: String,
    pub port: u16,
}

impl ProbeTask {
    /// Creates a normalized probe task from matrix expansion output.
    pub fn new(
        target: String,
        port: u16,
        protocol: String,
        source_role: String,
        target_name: String,
    ) -> Self {
        Self {
            target,
            port,
            protocol,
            source_role,
            target_name,
        }
    }

    /// Builds the stable cache key used to deduplicate tasks and store results.
    pub fn cache_key(&self) -> String {
        format!(
            "{}-{}-{}-{}-{}",
            self.protocol, self.source_role, self.target_name, self.target, self.port
        )
    }
}

impl ProbeResult {
    /// Creates a probe result from a task outcome and measured latency.
    pub fn from_task(task: &ProbeTask, success: bool, latency_ms: u128, timestamp: u64) -> Self {
        Self {
            success,
            latency_ms,
            last_probe_unix_seconds: timestamp,
            protocol: task.protocol.clone(),
            source_role: task.source_role.clone(),
            target: task.target.clone(),
            target_name: task.target_name.clone(),
            port: task.port,
        }
    }
}
