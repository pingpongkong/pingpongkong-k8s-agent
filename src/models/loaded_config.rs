use super::{DesiredNotificationState, DesiredPingState};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub published: PublishedConfig,
    pub desired_ping_state_yaml: String,
    pub notification_yamls: BTreeMap<String, String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PublishedConfig {
    pub desired_ping_state: DesiredPingState,
    pub desired_notification_state: DesiredNotificationState,
    pub revision: String,
    pub config_hash: String,
    pub synced_at: DateTime<Utc>,
}
