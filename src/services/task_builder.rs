use crate::infra::{discover_role_ips, get_my_roles, get_node_metadata};
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
    let role_ips = discover_role_ips(client.clone(), &config.topology).await?;
    let (_, node_ip_address) = get_node_metadata(client.clone(), node_name).await?;
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
                tasks.push(mark_self_probe(
                    ProbeTask::new(
                        target.clone(),
                        *port,
                        rule.protocol.as_str().to_string(),
                        rule.from.clone(),
                        rule.to.clone(),
                        rule.action,
                    ),
                    target.clone(),
                    &node_ip_address,
                ));
            }
        }
    }

    for rule in &config.matrix.external {
        if !my_roles.contains(&rule.from) {
            continue;
        }

        let (target, port) = parse_endpoint(&rule.endpoint)?;
        tasks.push(mark_self_probe(
            ProbeTask::new(
                target.clone(),
                port,
                rule.protocol.as_str().to_string(),
                rule.from.clone(),
                rule.name.clone().unwrap_or_else(|| rule.endpoint.clone()),
                rule.action,
            ),
            target,
            &node_ip_address,
        ));
    }

    tasks.sort_by_key(ProbeTask::cache_key);
    tasks.dedup();

    Ok(tasks)
}

fn mark_self_probe(task: ProbeTask, target_ip: String, node_ip: &str) -> ProbeTask {
    if target_ip.trim() == node_ip.trim() {
        task.with_message("does not test self to self")
    } else {
        task
    }
}
