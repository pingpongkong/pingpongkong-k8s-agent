use anyhow::Context;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub log_level: LogLevel,
    pub agent_check_interval: Duration,
    pub agent_api_port: u16,
    pub config_map: ConfigMapWatchOptions,
}

#[derive(Clone, Debug)]
pub struct ConfigMapWatchOptions {
    pub namespace: String,
    pub name: String,
    pub key: String,
    pub node_name: String,
}

#[derive(Clone, Copy, Debug)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl AppConfig {
    /// Creates application config from Kubernetes-friendly environment variables.
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            log_level: LogLevel::from_env("LOG_LEVEL")?,
            agent_check_interval: duration_from_env("AGENT_CHECK_INTERVAL", "5m")?,
            agent_api_port: port_from_env("AGENT_API_PORT", 8080)?,
            config_map: ConfigMapWatchOptions {
                namespace: std::env::var("CONFIG_NAMESPACE")
                    .unwrap_or_else(|_| "default".to_string()),
                name: std::env::var("CONFIGMAP_NAME")
                    .unwrap_or_else(|_| "pingpongkong-matrix".to_string()),
                key: std::env::var("CONFIGMAP_KEY")
                    .unwrap_or_else(|_| "desiredPingState.yaml".to_string()),
                node_name: required_env("NODE_NAME")?,
            },
        })
    }
}

impl LogLevel {
    fn from_env(name: &str) -> anyhow::Result<Self> {
        match std::env::var(name)
            .unwrap_or_else(|_| "INFO".to_string())
            .trim()
            .to_ascii_uppercase()
            .as_str()
        {
            "TRACE" => Ok(Self::Trace),
            "DEBUG" => Ok(Self::Debug),
            "INFO" => Ok(Self::Info),
            "WARN" => Ok(Self::Warn),
            "ERROR" => Ok(Self::Error),
            value => {
                anyhow::bail!("{name} must be one of TRACE, DEBUG, INFO, WARN, ERROR; got {value}")
            }
        }
    }
}

impl From<LogLevel> for tracing::Level {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Trace => tracing::Level::TRACE,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }
}

fn duration_from_env(name: &str, default: &str) -> anyhow::Result<Duration> {
    let raw = std::env::var(name).unwrap_or_else(|_| default.to_string());
    parse_duration(&raw).map_err(|err| anyhow::anyhow!("{name} {err}"))
}

fn parse_duration(raw: &str) -> anyhow::Result<Duration> {
    let raw = raw.trim();
    anyhow::ensure!(!raw.is_empty(), "cannot be empty");

    let split_at = raw
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(raw.len());
    let (amount, unit) = raw.split_at(split_at);
    anyhow::ensure!(!amount.is_empty(), "must start with a number");

    let amount = amount.parse::<u64>()?;
    anyhow::ensure!(amount > 0, "must be greater than zero");

    match unit {
        "s" => Ok(Duration::from_secs(amount)),
        "m" => Ok(Duration::from_secs(amount * 60)),
        "h" => Ok(Duration::from_secs(amount * 60 * 60)),
        _ => anyhow::bail!("must use a duration like 30s, 5m, or 1h"),
    }
}

fn port_from_env(name: &str, default: u16) -> anyhow::Result<u16> {
    let raw = std::env::var(name).unwrap_or_else(|_| default.to_string());
    let port = raw.trim().parse::<u16>()?;
    anyhow::ensure!(port > 0, "{name} must be between 1 and 65535");
    Ok(port)
}

fn required_env(name: &str) -> anyhow::Result<String> {
    std::env::var(name)
        .with_context(|| format!("{name} is required; set it to the Kubernetes node name"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_duration_examples() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn rejects_bad_duration() {
        assert!(parse_duration("30").is_err());
        assert!(parse_duration("0s").is_err());
        assert!(parse_duration("5d").is_err());
    }
}
