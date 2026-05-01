# PingPongKong Kubernetes Agent

`pingpongkong-k8s-agent` is the node-local execution agent for PingPongKong.
It runs as a Kubernetes DaemonSet, watches a desired-state ConfigMap with native
Kubernetes RBAC, expands that matrix into TCP and UDP probes, and exposes
the latest results on a local HTTP port.

## What It Does

- Watches `pingpongkong-{CONFIG_GIT_CLUSTERNAME}-ping-state` in `K8S_NAMESPACE`.
- Reads desired ping state YAML from the ConfigMap key `desiredPingState.yaml`.
- Resolves topology roles to Kubernetes node `InternalIP` addresses.
- Detects the role of the node running the current agent pod from `NODE_NAME`.
- Runs only the probes that apply to this node's role.
- Keeps the last good task set when a bad ConfigMap update is received.
- Restarts the ConfigMap watch stream after watch errors.
- Exposes Prometheus metrics at `:8080/metrics`.
- Exposes JSON debug state at `:8080/state`.

## Runtime Model

The agent does not need a Git token. Configuration is delivered through
Kubernetes itself:

1. Another component updates the desired-state ConfigMap.
2. The agent's service account watches that ConfigMap.
3. On each update, the agent validates the YAML and expands it into probe tasks.
4. Probe results are stored in memory and refreshed every probe cycle.
5. A collector or Prometheus scraper reads the agent's local HTTP endpoint.

## Configuration

The ConfigMap data value must be YAML matching this shape:

```yaml
version: "1.0"
cluster: example-cluster
topology:
  node-labels:
    controlplane: node-role.kubernetes.io/control-plane
    worker: node-role.kubernetes.io/worker
matrix:
  internal:
    - from: worker
      to: controlplane
      ports: [6443]
      protocol: tcp
      action: allow
    - from: controlplane
      to: worker
      ports: [10250]
      protocol: tcp
      action: deny
  external:
    - name: dns-google
      from: worker
      endpoint: 8.8.8.8:53
      protocol: udp
      action: allow
```

Supported protocols:

- `tcp`
- `udp`

External endpoints must use `host:port` form.

## Environment Variables

| Variable | Default | Description |
| --- | --- | --- |
| `K8S_NAMESPACE` | service account namespace | Namespace containing the desired-state ConfigMap. |
| `CONFIG_GIT_CLUSTERNAME` | required | Cluster name used to derive `pingpongkong-{CONFIG_GIT_CLUSTERNAME}-ping-state`. |
| `CONFIGMAP_KEY` | `desiredPingState.yaml` | Data key containing the desired ping state YAML. |
| `NODE_NAME` | required | Kubernetes node name for the current pod. Inject from `spec.nodeName`. |
| `LOG_LEVEL` | `INFO` | Log verbosity. One of `TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`. |
| `AGENT_CHECK_INTERVAL` | `5m` | Delay between probe cycles. Examples: `30s`, `5m`, `1h`. |
| `AGENT_API_PORT` | `8080` | HTTP API port used by the collector and metrics scrapers. |
| `PROBE_TIMEOUT_MILLISECONDS` | `3000` | Timeout for each probe attempt. |
| `PROBE_MAX_CONCURRENCY` | `512` | Maximum concurrent probe tasks per cycle. |

## HTTP Endpoints

### `GET /` or `GET /status`

Returns the node status shape consumed by the collector.

### `GET /metrics`

Returns Prometheus text exposition format.

Current metrics:

- `pingpongkong_active_probe_tasks`
- `pingpongkong_probe_success`
- `pingpongkong_probe_latency_milliseconds`

Example:

```text
pingpongkong_active_probe_tasks 2
pingpongkong_probe_success{protocol="tcp",source_role="worker",target="10.0.0.10",target_name="controlplane",port="6443"} 1
pingpongkong_probe_latency_milliseconds{protocol="tcp",source_role="worker",target="10.0.0.10",target_name="controlplane",port="6443"} 3
```

### `GET /state`

Returns the latest probe cache as JSON. This endpoint is intended for debugging
and local inspection.

## Kubernetes Permissions

RBAC is expected to be created by the Helm chart. The agent service account needs:

- `get`, `list`, `watch` on ConfigMaps in the configured namespace.
- `get`, `list` on Nodes cluster-wide.

Example RBAC sketch:

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: pingpongkong-agent-config
  namespace: default
rules:
  - apiGroups: [""]
    resources: ["configmaps"]
    verbs: ["get", "list", "watch"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: pingpongkong-agent-nodes
rules:
  - apiGroups: [""]
    resources: ["nodes"]
    verbs: ["get", "list"]
```

The DaemonSet should inject `NODE_NAME`:

```yaml
env:
  - name: K8S_NAMESPACE
    valueFrom:
      fieldRef:
        fieldPath: metadata.namespace
  - name: CONFIG_GIT_CLUSTERNAME
    value: sample-k8s-cluster
  - name: NODE_NAME
    valueFrom:
      fieldRef:
        fieldPath: spec.nodeName
```

ICMP probes may require `NET_RAW` capability depending on the cluster runtime
and pod security policy.

## Build

Requires Rust `1.95` or newer.

```bash
cargo build --release --locked
```

## Docker

Build the container image:

```bash
docker build -t pingpongkong-k8s-agent:local .
```

Run locally for a smoke test:

```bash
docker run --rm -p 8080:8080 \
  -e K8S_NAMESPACE=default \
  -e CONFIG_GIT_CLUSTERNAME=sample-k8s-cluster \
  -e NODE_NAME=test-node \
  pingpongkong-k8s-agent:local
```

Local Docker execution will not be able to watch Kubernetes unless it has a
valid kubeconfig or in-cluster service account environment.

## Development Checks

```bash
cargo fmt --check
cargo check
cargo build --release --locked
```

## Project Layout

```text
src/
  main.rs                  # Process entrypoint and task startup.
  configs/                 # Environment-backed runtime config.
  controllers/             # Axum HTTP controllers.
  errors/                  # Application error module.
  infra/                   # Kubernetes discovery and node metadata.
  models/                  # Split desired-state, probe, and API models.
  schedulers/              # Recurring probe scheduler.
  services/                # ConfigMap watch and task-building services.
```
