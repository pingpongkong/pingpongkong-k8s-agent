use super::task_builder::build_probe_tasks;
use crate::config::AgentConfig;
use crate::probe::{ProbeTask, SharedCache, SharedTasks};
use futures::StreamExt;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::{
    Client,
    api::Api,
    runtime::watcher::{Config as WatcherConfig, Event, watcher},
};
use std::collections::HashSet;
use tokio::time::{Duration, sleep};

#[derive(Clone, Debug)]
pub struct ConfigMapWatchOptions {
    pub namespace: String,
    pub name: String,
    pub key: String,
    pub node_name: String,
}

impl ConfigMapWatchOptions {
    /// Creates ConfigMap watch options from Kubernetes-friendly environment variables.
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            namespace: std::env::var("CONFIG_NAMESPACE").unwrap_or_else(|_| "default".to_string()),
            name: std::env::var("CONFIGMAP_NAME")
                .unwrap_or_else(|_| "pingpongkong-matrix".to_string()),
            key: std::env::var("CONFIGMAP_KEY").unwrap_or_else(|_| "matrix.yaml".to_string()),
            node_name: std::env::var("NODE_NAME")?,
        })
    }
}

/// Watches the matrix ConfigMap and keeps the shared desired task set up to date.
pub async fn run_config_watcher(
    options: ConfigMapWatchOptions,
    tasks: SharedTasks,
    cache: SharedCache,
) -> anyhow::Result<()> {
    let client = Client::try_default().await?;
    let configmaps: Api<ConfigMap> = Api::namespaced(client.clone(), &options.namespace);
    let watcher_config =
        WatcherConfig::default().fields(&format!("metadata.name={}", options.name));

    loop {
        let mut stream = watcher(configmaps.clone(), watcher_config.clone()).boxed();
        println!(
            "watching ConfigMap {}/{} key {}",
            options.namespace, options.name, options.key
        );

        while let Some(event) = stream.next().await {
            match event {
                Ok(Event::Applied(configmap)) => {
                    apply_configmap_update(&client, &options, &configmap, &tasks, &cache).await;
                }
                Ok(Event::Restarted(configmaps)) => {
                    for configmap in configmaps {
                        apply_configmap_update(&client, &options, &configmap, &tasks, &cache).await;
                    }
                }
                Ok(Event::Deleted(_)) => {
                    println!("watched ConfigMap was deleted; keeping last good task set");
                }
                Err(err) => {
                    eprintln!("ConfigMap watch error; restarting watch stream: {err:#}");
                    break;
                }
            }
        }

        sleep(Duration::from_secs(2)).await;
    }
}

/// Applies a ConfigMap update while preserving the last good task set on failures.
async fn apply_configmap_update(
    client: &Client,
    options: &ConfigMapWatchOptions,
    configmap: &ConfigMap,
    tasks: &SharedTasks,
    cache: &SharedCache,
) {
    if let Err(err) = reconcile_configmap(client, options, configmap, tasks, cache).await {
        eprintln!("failed to apply ConfigMap update; keeping last good tasks: {err:#}");
    }
}

/// Parses a ConfigMap, expands probe tasks, and prunes stale probe results.
async fn reconcile_configmap(
    client: &Client,
    options: &ConfigMapWatchOptions,
    configmap: &ConfigMap,
    tasks: &SharedTasks,
    cache: &SharedCache,
) -> anyhow::Result<()> {
    let Some(data) = &configmap.data else {
        anyhow::bail!("ConfigMap has no data");
    };

    let Some(raw_config) = data.get(&options.key) else {
        anyhow::bail!("ConfigMap is missing key {}", options.key);
    };

    let config = AgentConfig::load_from_str(raw_config)?;
    let next_tasks = build_probe_tasks(client.clone(), &options.node_name, &config).await?;
    let task_count = next_tasks.len();
    let next_task_keys: HashSet<String> = next_tasks.iter().map(ProbeTask::cache_key).collect();

    *tasks.write().await = next_tasks;
    cache
        .write()
        .await
        .retain(|cache_key, _| next_task_keys.contains(cache_key));

    println!(
        "loaded config for cluster {} with {task_count} probe tasks",
        config.cluster
    );

    Ok(())
}
