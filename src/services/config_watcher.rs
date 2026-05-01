use super::build_probe_tasks;
use crate::configs::ConfigMapWatchOptions;
use crate::models::{DesiredPingState, ProbeTask, SharedCache, SharedTasks};
use futures::StreamExt;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::{
    Client,
    api::Api,
    runtime::watcher::{Config as WatcherConfig, Event, watcher},
};
use std::collections::HashSet;
use tokio::time::{Duration, sleep};
use tracing::{error, info, warn};

/// Watches the desired-state ConfigMap and keeps the shared task set up to date.
pub async fn run_config_watcher(
    options: ConfigMapWatchOptions,
    tasks: SharedTasks,
    cache: SharedCache,
) -> anyhow::Result<()> {
    let client = Client::try_default().await?;
    let configmaps: Api<ConfigMap> = Api::namespaced(client.clone(), &options.namespace);
    let watcher_config =
        WatcherConfig::default().fields(&format!("metadata.name={}", options.name));
    log_initial_configmap_state(&configmaps, &options, &tasks, &cache).await;

    loop {
        let mut stream = watcher(configmaps.clone(), watcher_config.clone()).boxed();
        info!(
            namespace = %options.namespace,
            config_map = %options.name,
            key = %options.key,
            node_name = %options.node_name,
            "watching desired-state ConfigMap"
        );

        while let Some(event) = stream.next().await {
            match event {
                Ok(Event::Apply(configmap) | Event::InitApply(configmap)) => {
                    apply_configmap_update(&client, &options, &configmap, &tasks, &cache).await;
                }
                Ok(Event::Delete(_)) => {
                    clear_probe_state(&tasks, &cache).await;
                    warn!(
                        namespace = %options.namespace,
                        config_map = %options.name,
                        "desired-state ConfigMap was deleted; cleared probe tasks and paused probing"
                    );
                }
                Ok(Event::Init | Event::InitDone) => {}
                Err(err) => {
                    error!(
                        namespace = %options.namespace,
                        config_map = %options.name,
                        error = %err,
                        "ConfigMap watch failed; retrying in 2s"
                    );
                    break;
                }
            }
        }

        sleep(Duration::from_secs(2)).await;
    }
}

/// Applies a ConfigMap update and pauses probing when the desired state is unusable.
async fn apply_configmap_update(
    client: &Client,
    options: &ConfigMapWatchOptions,
    configmap: &ConfigMap,
    tasks: &SharedTasks,
    cache: &SharedCache,
) {
    if let Err(err) = reconcile_configmap(client, options, configmap, tasks, cache).await {
        clear_probe_state(tasks, cache).await;
        error!(
            namespace = %options.namespace,
            config_map = %options.name,
            key = %options.key,
            error = %err,
            "failed to apply desired-state ConfigMap update; cleared probe tasks and paused probing"
        );
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

    let config = DesiredPingState::load_from_str(raw_config)?;
    let next_tasks = build_probe_tasks(client.clone(), &options.node_name, &config).await?;
    let task_count = next_tasks.len();
    let next_task_keys: HashSet<String> = next_tasks.iter().map(ProbeTask::cache_key).collect();

    *tasks.write().await = next_tasks;
    cache
        .write()
        .await
        .retain(|cache_key, _| next_task_keys.contains(cache_key));

    info!(
        cluster = %config.cluster,
        version = %config.version,
        task_count,
        internal_rules = config.matrix.internal.len(),
        external_rules = config.matrix.external.len(),
        "accepted desired ping state from ConfigMap"
    );

    Ok(())
}

async fn clear_probe_state(tasks: &SharedTasks, cache: &SharedCache) {
    tasks.write().await.clear();
    cache.write().await.clear();
}

async fn log_initial_configmap_state(
    configmaps: &Api<ConfigMap>,
    options: &ConfigMapWatchOptions,
    tasks: &SharedTasks,
    cache: &SharedCache,
) {
    match configmaps.get(&options.name).await {
        Ok(_) => {}
        Err(kube::Error::Api(response)) if response.code == 404 => {
            clear_probe_state(tasks, cache).await;
            warn!(
                namespace = %options.namespace,
                config_map = %options.name,
                key = %options.key,
                "desired-state ConfigMap does not exist; probing is paused until it is created"
            );
        }
        Err(err) => {
            warn!(
                namespace = %options.namespace,
                config_map = %options.name,
                error = %err,
                "could not check desired-state ConfigMap before starting watch"
            );
        }
    }
}
