use crate::models::Topology;
use k8s_openapi::api::core::v1::Node;
use kube::{Api, Client, api::ListParams};
use std::collections::BTreeMap;

/// Maps PingPongKong topology aliases to Kubernetes node InternalIP addresses.
pub async fn discover_role_ips(
    client: Client,
    topology: &Topology,
) -> anyhow::Result<BTreeMap<String, Vec<String>>> {
    let nodes: Api<Node> = Api::all(client);
    let node_list = nodes.list(&ListParams::default()).await?;
    let mut role_to_ips = empty_role_map(topology);

    for node in node_list.items {
        let labels = node.metadata.labels.clone().unwrap_or_default();
        let Some(ip) = node_internal_ip(&node) else {
            continue;
        };

        for (role_name, role_label) in &topology.node_labels {
            if labels.contains_key(role_label)
                && let Some(ip_list) = role_to_ips.get_mut(role_name)
            {
                ip_list.push(ip.clone());
            }
        }
    }

    Ok(role_to_ips)
}

/// Resolves the PingPongKong topology aliases for the node hosting this agent pod.
pub async fn get_my_roles(
    client: Client,
    my_node_name: &str,
    topology: &Topology,
) -> anyhow::Result<Vec<String>> {
    let nodes: Api<Node> = Api::all(client);
    let my_node = nodes.get(my_node_name).await?;
    let labels = my_node.metadata.labels.unwrap_or_default();

    let mut my_roles = Vec::new();

    for (role_name, role_label) in &topology.node_labels {
        if labels.contains_key(role_label) {
            my_roles.push(role_name.clone());
        }
    }

    Ok(my_roles)
}

pub async fn get_node_metadata(
    client: Client,
    node_name: &str,
) -> anyhow::Result<(BTreeMap<String, String>, String)> {
    let nodes: Api<Node> = Api::all(client);
    let node = nodes.get(node_name).await?;
    let labels = node
        .metadata
        .labels
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let ip_address = node_internal_ip(&node).unwrap_or_default();

    Ok((labels, ip_address))
}

/// Creates an empty alias-to-IP map for all aliases declared by the config topology.
fn empty_role_map(topology: &Topology) -> BTreeMap<String, Vec<String>> {
    topology
        .node_labels
        .keys()
        .map(|role_name| (role_name.clone(), Vec::new()))
        .collect()
}

/// Extracts the preferred InternalIP address from a Kubernetes node object.
fn node_internal_ip(node: &Node) -> Option<String> {
    node.status
        .as_ref()?
        .addresses
        .as_ref()?
        .iter()
        .find(|address| address.type_ == "InternalIP")
        .map(|address| address.address.clone())
}
