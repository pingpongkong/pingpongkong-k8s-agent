use super::ProbeResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub node_name: String,
    pub labels: BTreeMap<String, String>,
    pub ip_address: String,
    pub health_status: NodeHealthStatus,
    pub targets: Vec<TargetStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeHealthStatus {
    Healthy,
    Unreachable,
    Degraded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetStatus {
    pub target_name: String,
    pub target_ip_address: String,
    pub health_status: TargetHealthStatus,
    pub latency_ms: Option<u64>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TargetHealthStatus {
    Healthy,
    Unreachable,
    Failed,
}

impl NodeStatus {
    pub fn from_probe_results(
        node_name: String,
        labels: BTreeMap<String, String>,
        ip_address: String,
        results: Vec<ProbeResult>,
    ) -> Self {
        let targets = results
            .into_iter()
            .map(|result| TargetStatus::from_probe_result(result, &ip_address))
            .collect::<Vec<_>>();
        let health_status = if targets
            .iter()
            .all(|target| target.health_status == TargetHealthStatus::Healthy)
        {
            NodeHealthStatus::Healthy
        } else {
            NodeHealthStatus::Degraded
        };

        Self {
            node_name,
            labels,
            ip_address,
            health_status,
            targets,
        }
    }
}

impl TargetStatus {
    fn from_probe_result(result: ProbeResult, node_ip_address: &str) -> Self {
        let health_status = result.target_health_status();
        let error_message = result.error_message();
        let latency_ms = u64::try_from(result.latency_ms).ok();
        let target_ip_address = result.target;

        if target_ip_address.trim() == node_ip_address.trim() {
            return Self {
                target_name: result.target_name,
                target_ip_address,
                health_status: TargetHealthStatus::Healthy,
                latency_ms: Some(0),
                error_message: Some("does not test self to self".to_string()),
            };
        }

        Self {
            target_name: result.target_name,
            target_ip_address,
            health_status,
            latency_ms,
            error_message,
        }
    }
}
