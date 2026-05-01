use super::{PingRuleAction, TargetHealthStatus};
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
    pub action: PingRuleAction,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub success: bool,
    pub expected_success: bool,
    pub latency_ms: u128,
    pub last_probe_unix_seconds: u64,
    pub protocol: String,
    pub source_role: String,
    pub target: String,
    pub target_name: String,
    pub port: u16,
    pub action: PingRuleAction,
    pub message: String,
}

impl ProbeTask {
    /// Creates a normalized probe task from matrix expansion output.
    pub fn new(
        target: String,
        port: u16,
        protocol: String,
        source_role: String,
        target_name: String,
        action: PingRuleAction,
    ) -> Self {
        Self {
            target,
            port,
            protocol,
            source_role,
            target_name,
            action,
            message: String::new(),
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    /// Builds the stable cache key used to deduplicate tasks and store results.
    pub fn cache_key(&self) -> String {
        format!(
            "{}-{}-{}-{}-{}-{:?}",
            self.protocol, self.source_role, self.target_name, self.target, self.port, self.action
        )
    }
}

impl ProbeResult {
    /// Creates a probe result from a task outcome and measured latency.
    pub fn from_task(task: &ProbeTask, success: bool, latency_ms: u128, timestamp: u64) -> Self {
        Self {
            success,
            expected_success: task.action.expected_success(),
            latency_ms,
            last_probe_unix_seconds: timestamp,
            protocol: task.protocol.clone(),
            source_role: task.source_role.clone(),
            target: task.target.clone(),
            target_name: task.target_name.clone(),
            port: task.port,
            action: task.action,
            message: task.message.clone(),
        }
    }

    pub fn target_health_status(&self) -> TargetHealthStatus {
        if !self.message.is_empty() {
            return TargetHealthStatus::Healthy;
        }

        match (self.expected_success, self.success) {
            (true, true) | (false, false) => TargetHealthStatus::Healthy,
            (true, false) => TargetHealthStatus::Unreachable,
            (false, true) => TargetHealthStatus::Failed,
        }
    }

    pub fn error_message(&self) -> Option<String> {
        if !self.message.is_empty() {
            return Some(self.message.clone());
        }

        match (self.expected_success, self.success) {
            (true, false) => Some("target was unreachable".to_string()),
            (false, true) => Some("target was reachable but rule action is deny".to_string()),
            _ => None,
        }
    }
}
