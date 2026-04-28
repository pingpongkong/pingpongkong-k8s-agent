use super::{ProbeResult, ProbeTask, SharedCache, SharedTasks};
use futures::{StreamExt, stream::FuturesUnordered};
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use surge_ping::{Client, Config, PingIdentifier, PingSequence};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Semaphore;
use tokio::time::{Duration, Instant, sleep, timeout};

/// Runs the recurring probe cycle using the latest desired task snapshot.
pub async fn run_probe_loop(tasks: SharedTasks, cache: SharedCache) {
    let interval = env_u64("PROBE_INTERVAL_SECONDS", 15);
    let timeout_duration = Duration::from_millis(env_u64("PROBE_TIMEOUT_MILLISECONDS", 3000));
    let max_concurrency = env_usize("PROBE_MAX_CONCURRENCY", 512).max(1);

    loop {
        let current_tasks = tasks.read().await.clone();

        if current_tasks.is_empty() {
            println!("no probe tasks loaded yet");
        } else {
            println!(
                "starting probe cycle with {} tasks and max concurrency {}",
                current_tasks.len(),
                max_concurrency
            );
            run_all_probes(
                current_tasks,
                Arc::clone(&cache),
                timeout_duration,
                max_concurrency,
            )
            .await;
        }

        sleep(Duration::from_secs(interval)).await;
    }
}

/// Runs one bounded-concurrency probe batch and writes all latest results into cache.
pub async fn run_all_probes(
    tasks: Vec<ProbeTask>,
    cache: SharedCache,
    timeout_duration: Duration,
    max_concurrency: usize,
) {
    let semaphore = Arc::new(Semaphore::new(max_concurrency));
    let mut pending = FuturesUnordered::new();

    for task in tasks {
        let cache_ref = Arc::clone(&cache);
        let semaphore_ref = Arc::clone(&semaphore);

        pending.push(tokio::spawn(async move {
            let Ok(_permit) = semaphore_ref.acquire_owned().await else {
                return;
            };

            let result = run_single_probe(&task, timeout_duration).await;
            cache_ref.write().await.insert(task.cache_key(), result);
        }));
    }

    while let Some(join_result) = pending.next().await {
        if let Err(err) = join_result {
            eprintln!("probe task join error: {err}");
        }
    }
}

/// Executes one probe task and converts its result into the shared cache format.
async fn run_single_probe(task: &ProbeTask, timeout_duration: Duration) -> ProbeResult {
    let start = Instant::now();
    let success = match task.protocol.as_str() {
        "tcp" => probe_tcp(&task.target, task.port, timeout_duration).await,
        "udp" => probe_udp(&task.target, task.port, timeout_duration).await,
        "icmp" => probe_icmp(&task.target, timeout_duration).await,
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
