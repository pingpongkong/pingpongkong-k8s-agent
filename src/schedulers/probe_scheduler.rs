use crate::models::{ProbeResult, ProbeTask, SharedCache, SharedTasks, TargetHealthStatus};
use futures::{StreamExt, stream::FuturesUnordered};
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use surge_ping::{Client, Config, PingIdentifier, PingSequence};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Semaphore;
use tokio::time::{Instant, sleep, timeout};
use tracing::{error, info};

#[derive(Default)]
pub struct ProbeBatchSummary {
    pub total: usize,
    pub healthy: usize,
    pub unreachable: usize,
    pub failed: usize,
}

/// Runs the recurring probe cycle using the latest desired task snapshot.
pub async fn run_probe_loop(tasks: SharedTasks, cache: SharedCache, interval: Duration) {
    let timeout_duration = Duration::from_millis(env_u64("PROBE_TIMEOUT_MILLISECONDS", 3000));
    let max_concurrency = env_usize("PROBE_MAX_CONCURRENCY", 512).max(1);
    info!(
        interval = %format_duration(interval),
        timeout_ms = timeout_duration.as_millis(),
        max_concurrency,
        "probe scheduler started"
    );

    loop {
        let current_tasks = tasks.read().await.clone();

        if current_tasks.is_empty() {
            info!(
                next_check_in = %format_duration(interval),
                "no probe tasks loaded yet; waiting for desired state"
            );
        } else {
            let task_count = current_tasks.len();
            info!(
                task_count,
                max_concurrency,
                timeout_ms = timeout_duration.as_millis(),
                "starting probe cycle"
            );
            let summary = run_all_probes(
                current_tasks,
                Arc::clone(&cache),
                timeout_duration,
                max_concurrency,
            )
            .await;
            info!(
                task_count = summary.total,
                healthy = summary.healthy,
                unreachable = summary.unreachable,
                failed = summary.failed,
                next_check_in = %format_duration(interval),
                "probe cycle completed"
            );
        }

        sleep(interval).await;
    }
}

/// Runs one bounded-concurrency probe batch and writes all latest results into cache.
pub async fn run_all_probes(
    tasks: Vec<ProbeTask>,
    cache: SharedCache,
    timeout_duration: Duration,
    max_concurrency: usize,
) -> ProbeBatchSummary {
    let semaphore = Arc::new(Semaphore::new(max_concurrency));
    let mut pending = FuturesUnordered::new();
    let mut summary = ProbeBatchSummary::default();

    for task in tasks {
        let cache_ref = Arc::clone(&cache);
        let semaphore_ref = Arc::clone(&semaphore);

        pending.push(tokio::spawn(async move {
            let Ok(_permit) = semaphore_ref.acquire_owned().await else {
                return None;
            };

            let result = run_single_probe(&task, timeout_duration).await;
            cache_ref
                .write()
                .await
                .insert(task.cache_key(), result.clone());
            Some(result)
        }));
    }

    while let Some(join_result) = pending.next().await {
        match join_result {
            Ok(Some(result)) => summary.record(&result),
            Ok(None) => {}
            Err(err) => error!(error = %err, "probe task join error"),
        }
    }

    summary
}

/// Executes one probe task and converts its result into the shared cache format.
async fn run_single_probe(task: &ProbeTask, timeout_duration: Duration) -> ProbeResult {
    let start = Instant::now();
    let success = match task.protocol.as_str() {
        "tcp" => probe_tcp(&task.target, task.port, timeout_duration).await,
        "udp" => probe_udp(&task.target, task.port, timeout_duration).await,
        _ => false,
    };

    ProbeResult::from_task(task, success, start.elapsed().as_millis(), unix_timestamp())
}

/// Checks whether a TCP connection can be established before the timeout.
async fn probe_tcp(target: &str, port: u16, timeout_duration: Duration) -> bool {
    let addr = format!("{}:{}", target, port);
    matches!(
        timeout(timeout_duration, TcpStream::connect(&addr)).await,
        Ok(Ok(_))
    )
}

/// Checks whether a UDP datagram can be sent before the timeout.
async fn probe_udp(target: &str, port: u16, timeout_duration: Duration) -> bool {
    let addr = format!("{}:{}", target, port);
    let Ok(socket) = UdpSocket::bind("0.0.0.0:0").await else {
        return false;
    };

    matches!(
        timeout(timeout_duration, socket.send_to(b"", &addr)).await,
        Ok(Ok(_))
    )
}

#[allow(dead_code)]
/// Checks whether an ICMP echo succeeds before the timeout.
async fn probe_icmp(target: &str, timeout_duration: Duration) -> bool {
    let Ok(ip) = IpAddr::from_str(target) else {
        return false;
    };

    let Ok(client) = Client::new(&Config::default()) else {
        return false;
    };

    let mut pinger = client.pinger(ip, PingIdentifier(111)).await;
    matches!(
        timeout(
            timeout_duration,
            pinger.ping(PingSequence(0), b"PingPongKong")
        )
        .await,
        Ok(Ok(_))
    )
}

/// Reads an unsigned integer environment variable or returns a default value.
fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(default)
}

/// Reads an unsigned size environment variable or returns a default value.
fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(default)
}

/// Returns the current UNIX timestamp in seconds.
fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

impl ProbeBatchSummary {
    fn record(&mut self, result: &ProbeResult) {
        self.total += 1;
        match result.target_health_status() {
            TargetHealthStatus::Healthy => self.healthy += 1,
            TargetHealthStatus::Unreachable => self.unreachable += 1,
            TargetHealthStatus::Failed => self.failed += 1,
        }
    }
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();

    if seconds == 0 {
        format!("{}ms", duration.as_millis())
    } else if seconds % 3600 == 0 {
        format!("{}h", seconds / 3600)
    } else if seconds % 60 == 0 {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}
