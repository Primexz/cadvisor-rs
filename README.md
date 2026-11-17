<p align="center">
  <img src="assets/cadvisor-rs-logo.png" width="160" height="160" alt="Ferris, the Rust crab mascot, holding a container metrics display">
</p>

<h1 align="center">cadvisor-rs</h1>

<p align="center">
  Container metrics for Prometheus, written in Rust.
</p>

<p align="center">
  <a href="#quick-start"><img src="https://img.shields.io/badge/Linux-amd64%20%7C%20arm64-334155?style=for-the-badge&amp;logo=linux&amp;logoColor=white&amp;labelColor=1e293b" alt="Linux amd64 and arm64"></a>
  <a href="#development"><img src="https://img.shields.io/badge/Rust-1.90%2B-CE422B?style=for-the-badge&amp;logo=rust&amp;logoColor=white&amp;labelColor=1e293b" alt="Rust 1.90 or newer"></a>
  <a href="#prometheus"><img src="https://img.shields.io/badge/Prometheus-metrics-E6522C?style=for-the-badge&amp;logo=prometheus&amp;logoColor=white&amp;labelColor=1e293b" alt="Prometheus metrics"></a>
  <a href="#grafana"><img src="https://img.shields.io/badge/Grafana-dashboard-F46800?style=for-the-badge&amp;logo=grafana&amp;logoColor=white&amp;labelColor=1e293b" alt="Grafana dashboard"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-2563EB?style=for-the-badge&amp;labelColor=1e293b" alt="MIT license"></a>
</p>

## Features

- 156 metric families for CPU, memory, disk I/O, network, processes and pressure stalls.
- Docker health checks, restarts, exit codes, uptime and stopped-container inventory.
- Filesystem capacity, inodes and optional writable-layer usage.
- cAdvisor-compatible Prometheus resource metrics and a JSON snapshot API.
- A Grafana dashboard with 30 panels and host/container filters.
- Direct cgroup collection with cached responses shared across scrapers.

Supports Linux amd64 and arm64 with cgroup v1 or v2. Docker adds names, images,
labels, health and lifecycle information.

## Quick start

Requires a Linux Docker host and Docker Compose. From the repository checkout:

```sh
docker compose up -d --build
curl http://localhost:8080/metrics
curl http://localhost:8080/readyz
```

[compose.yaml](compose.yaml) binds the exporter to `127.0.0.1:8080`. It uses the
host cgroup/PID namespaces, read-only cgroup and procfs mounts, and `SYS_PTRACE`
for process and filesystem visibility. The Docker socket supplies metadata;
read-only mounting does not restrict which Docker API operations the socket permits.

To run the binary directly on Linux:

```sh
cargo build --release --locked
sudo ./target/release/cadvisor-rs --listen 127.0.0.1:8080 --containers-only
```

## Prometheus

Add the exporter to your scrape configuration:

```yaml
scrape_configs:
  - job_name: cadvisor-rs
    static_configs:
      - targets: ["localhost:8080"]
```

Use the exporter's reachable hostname when Prometheus runs elsewhere; `localhost`
only works when both share the same host network. See the [example configuration](examples/prometheus.yml)
and [alert rules](examples/alerts.yml).

## Grafana

Open **Dashboards → New → Import** and upload
[grafana/cadvisor-rs.json](grafana/cadvisor-rs.json). Select your Prometheus
source, job, hosts and containers at the top of the dashboard.

The dashboard covers resource usage, health, restarts, connections, filesystems
and exporter diagnostics.

## Configuration

Set environment variables on the Compose service, for example:

```yaml
services:
  cadvisor-rs:
    environment:
      CADVISOR_RS_INTERVAL_SECONDS: "5"
      CADVISOR_RS_DISCOVERY_INTERVAL_SECONDS: "15"
      CADVISOR_RS_DOCKER_LABELS: "com.docker.compose.service"
```

Resource collection defaults to 10 seconds; Docker metadata and cgroup discovery
refresh every 30 seconds. Enable `CADVISOR_RS_DOCKER_SIZE_METRICS=true` for
writable-layer sizes; Docker may scan disk to collect them. Set
`CADVISOR_RS_DISABLE_DOCKER=true` and omit the socket mount for cgroup collection
without Docker enrichment.

Run `cadvisor-rs --help` for all flags and environment variables.

| Endpoint | Purpose |
| --- | --- |
| `/metrics` | Cached Prometheus metrics |
| `/healthz` | Exporter liveness |
| `/readyz` | Fresh, successful core collection |
| `/api/v1/containers` | Cached JSON snapshot |

## Performance

| Exporter | RSS (MiB) | CPU (% of one core) | Scrape p50 (ms) | Scrape p95 (ms) |
| --- | ---: | ---: | ---: | ---: |
| cAdvisor v0.56.2 | 50.47 | 43.35 | 38.00 | 57.15 |
| cadvisor-rs | 5.74 | 1.45 | 3.91 | 6.39 |

## Development

The pinned toolchain includes rustfmt and Clippy.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --locked
```

## License

[MIT](LICENSE) © Primexz 2025. Inspired by [Google cAdvisor](https://github.com/google/cadvisor);
this project independently implements Linux collection and compatible metric names.
