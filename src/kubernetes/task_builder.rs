use super::discovery::{discover_role_ips, get_my_roles};
use crate::config::{AgentConfig, parse_endpoint};
use crate::probe::ProbeTask;
use kube::Client;
use std::collections::HashSet;

/// Expands the current matrix and node role into concrete probe tasks.
pub async fn build_probe_tasks(
    client: Client,
    node_name: &str,
    config: &AgentConfig,
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
                    rule.proto.clone(),
                    rule.from.clone(),
                    rule.to.clone(),
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
            rule.proto.clone(),
            rule.from.clone(),
            rule.name.clone(),
        ));
    }

    tasks.sort_by_key(ProbeTask::cache_key);
    tasks.dedup();

    Ok(tasks)
}
