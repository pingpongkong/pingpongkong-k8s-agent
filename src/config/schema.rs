use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub version: String,
    pub cluster: String,
    pub topology: Topology,
    pub matrix: Matrix,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Topology {
    pub roles: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Matrix {
    pub internal: Vec<InternalRule>,
    pub external: Vec<ExternalRule>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InternalRule {
    pub from: String,
    pub to: String,
    pub ports: Vec<u16>,
    pub proto: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExternalRule {
    pub name: String,
    pub from: String,
    pub endpoint: String,
    pub proto: String,
}

impl AgentConfig {
    /// Parses and validates an agent configuration document from ConfigMap YAML.
    pub fn load_from_str(contents: &str) -> anyhow::Result<Self> {
        let config: AgentConfig = serde_yaml::from_str(contents)?;
        config.validate()?;
        Ok(config)
    }

    /// Verifies that the matrix references known roles and supported protocols.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.version.trim().is_empty() {
            anyhow::bail!("config version must not be empty");
        }

        if self.cluster.trim().is_empty() {
            anyhow::bail!("cluster must not be empty");
        }

        if self.topology.roles.is_empty() {
            anyhow::bail!("topology.roles must define at least one role");
        }

        for rule in &self.matrix.internal {
            validate_role(&self.topology, &rule.from)?;
            validate_role(&self.topology, &rule.to)?;
            validate_proto(&rule.proto)?;

            if rule.ports.is_empty() {
                anyhow::bail!(
                    "internal rule {} -> {} must define at least one port",
                    rule.from,
                    rule.to
                );
            }
        }

        for rule in &self.matrix.external {
            validate_role(&self.topology, &rule.from)?;
            validate_proto(&rule.proto)?;
            parse_endpoint(&rule.endpoint).map_err(|err| {
                anyhow::anyhow!(
                    "external rule {} has invalid endpoint {}: {err}",
                    rule.name,
                    rule.endpoint
                )
            })?;
        }

        Ok(())
    }
}

/// Splits a matrix endpoint in host:port form into its host and numeric port.
pub fn parse_endpoint(endpoint: &str) -> anyhow::Result<(String, u16)> {
    let (host, port) = endpoint
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("expected host:port"))?;

    if host.trim().is_empty() {
        anyhow::bail!("host must not be empty");
    }

    let port = port.parse::<u16>()?;

    Ok((host.to_string(), port))
}

/// Confirms that a rule role exists in the topology section.
fn validate_role(topology: &Topology, role: &str) -> anyhow::Result<()> {
    if !topology.roles.contains_key(role) {
        anyhow::bail!("unknown role: {role}");
    }

    Ok(())
}

/// Confirms that the probe protocol is supported by the agent.
fn validate_proto(proto: &str) -> anyhow::Result<()> {
    match proto {
        "tcp" | "udp" | "icmp" => Ok(()),
        _ => anyhow::bail!("unsupported protocol: {proto}"),
    }
}
