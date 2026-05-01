use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct DesiredNotificationState {
    #[serde(flatten)]
    pub values: BTreeMap<String, serde_json::Value>,
}
