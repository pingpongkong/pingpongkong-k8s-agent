use crate::infra::{discover_role_ips, get_my_roles};
use crate::models::{DesiredPingState, ProbeTask, parse_endpoint};
use kube::Client;
use std::collections::HashSet;

/// Expands the current desired state and node role into concrete probe tasks.
pub async fn build_probe_tasks(
    client: Client,
    node_name: &str,
    config: &DesiredPingState,
) -> anyhow::Result<Vec<ProbeTask>> {
    let my_roles: HashSet<String> = get_my_roles(client.clone(), node_name, &config.topology)
        .await?
        .into_iter()
        .collect();
    let role_ips = discover_role_ips(client, &config.topology).await?;
    let mut tasks = Vec::new();

    for rule in &config.matrix.internal {
        if !my_roles.contains(&rule.from) {
            continue;
        }

        let Some(target_ips) = role_ips.get(&rule.to) else {
            continue;
        };

        for target in target_ips {
            for port in &rule.ports {
                tasks.push(ProbeTask::new(
                    target.clone(),
                    *port,
                    rule.protocol.as_str().to_string(),
                    rule.from.clone(),
                    rule.to.clone(),
                    rule.action,
                ));
            }
        }
    }

    for rule in &config.matrix.external {
        if !my_roles.contains(&rule.from) {
            continue;
        }

        let (target, port) = parse_endpoint(&rule.endpoint)?;
        tasks.push(ProbeTask::new(
            target,
            port,
            rule.protocol.as_str().to_string(),
            rule.from.clone(),
            rule.name.clone().unwrap_or_else(|| rule.endpoint.clone()),
            rule.action,
        ));
    }

    tasks.sort_by_key(ProbeTask::cache_key);
    tasks.dedup();

    Ok(tasks)
}
