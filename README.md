# PingPongKong Kubernetes Agent

`pingpongkong-k8s-agent` is the node-local execution agent for PingPongKong.
It runs as a Kubernetes DaemonSet, watches a matrix ConfigMap with native
Kubernetes RBAC, expands that matrix into TCP, UDP, and ICMP probes, and exposes
the latest results on a local HTTP port.

## What It Does

- Watches a ConfigMap, by default `default/pingpongkong-matrix`.
- Reads the matrix YAML from the ConfigMap key `matrix.yaml`.
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

1. Another component updates the matrix ConfigMap.
2. The agent's service account watches that ConfigMap.
3. On each update, the agent validates the YAML and expands it into probe tasks.
4. Probe results are stored in memory and refreshed every probe cycle.
5. A collector or Prometheus scraper reads the agent's local HTTP endpoint.

## Configuration

The ConfigMap data value must be YAML matching this shape:

```yaml
version: v1
cluster: example-cluster
topology:
  roles:
    controlplane: node-role.kubernetes.io/control-plane
    worker: node-role.kubernetes.io/worker
matrix:
  internal:
    - from: worker
      to: controlplane
      ports: [6443]
      proto: tcp
    - from: controlplane
      to: worker
      ports: [10250]
      proto: tcp
  external:
    - name: dns-google
      from: worker
      endpoint: 8.8.8.8:53
      proto: udp
```

Supported protocols:

- `tcp`
- `udp`
- `icmp`

External endpoints must use `host:port` form.

## Environment Variables

| Variable | Default | Description |
| --- | --- | --- |
| `CONFIG_NAMESPACE` | `default` | Namespace containing the matrix ConfigMap. |
| `CONFIGMAP_NAME` | `pingpongkong-matrix` | Name of the watched ConfigMap. |
| `CONFIGMAP_KEY` | `matrix.yaml` | Data key containing the matrix YAML. |
| `NODE_NAME` | required | Kubernetes node name for the current pod. Inject from `spec.nodeName`. |
| `PROBE_INTERVAL_SECONDS` | `15` | Delay between probe cycles. |
| `PROBE_TIMEOUT_MILLISECONDS` | `3000` | Timeout for each probe attempt. |
| `PROBE_MAX_CONCURRENCY` | `512` | Maximum concurrent probe tasks per cycle. |

## HTTP Endpoints

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

The agent service account needs:

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
  - name: NODE_NAME
    valueFrom:
      fieldRef:
        fieldPath: spec.nodeName
```

ICMP probes may require `NET_RAW` capability depending on the cluster runtime
and pod security policy.

## Build

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
  server.rs                # Axum HTTP routes for metrics and JSON state.
  config/
    mod.rs
    schema.rs              # Matrix schema, parsing, and validation.
  kubernetes/
    mod.rs
    discovery.rs           # Node role and InternalIP discovery.
    task_builder.rs        # Matrix-to-probe task expansion.
    watcher.rs             # ConfigMap watch and reconciliation.
  probe/
    mod.rs
    runner.rs              # Bounded-concurrency probe engine.
    types.rs               # Probe task/result shared types.
```
