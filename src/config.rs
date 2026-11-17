//! Command-line and environment configuration.

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use clap::Parser;

/// Export Linux container metrics using bounded, periodic collection.
#[derive(Clone, Debug, Parser)]
#[command(version, about)]
pub struct Config {
    /// Address for the HTTP listener.
    #[arg(long, env = "CADVISOR_RS_LISTEN", default_value = "0.0.0.0:8080")]
    pub listen: SocketAddr,
    /// Host cgroup filesystem, with v1 controller mounts or a unified v2 mount.
    #[arg(
        long,
        env = "CADVISOR_RS_CGROUP_ROOT",
        default_value = "/sys/fs/cgroup"
    )]
    pub cgroup_root: PathBuf,
    /// Host procfs; mount the host /proc here to collect container network data.
    #[arg(long, env = "CADVISOR_RS_PROC_ROOT", default_value = "/proc")]
    pub proc_root: PathBuf,
    /// Docker Unix socket for names, images, labels and health/lifecycle status.
    #[arg(
        long,
        env = "CADVISOR_RS_DOCKER_SOCKET",
        default_value = "/var/run/docker.sock"
    )]
    pub docker_socket: PathBuf,
    /// Disable Docker metadata requests (raw cgroup metrics still work).
    #[arg(long, env = "CADVISOR_RS_DISABLE_DOCKER")]
    pub disable_docker: bool,
    /// Disable bounded Docker inspect requests (omit restart/start/exit/config metrics).
    #[arg(long, env = "CADVISOR_RS_DISABLE_DOCKER_INSPECT")]
    pub disable_docker_inspect: bool,
    /// Include only running Docker containers in the inventory.
    #[arg(long, env = "CADVISOR_RS_DOCKER_RUNNING_ONLY")]
    pub docker_running_only: bool,
    /// Request writable-layer/image sizes from Docker; this can trigger disk scans.
    #[arg(long, env = "CADVISOR_RS_DOCKER_SIZE_METRICS")]
    pub docker_size_metrics: bool,
    /// Disable process, thread, file descriptor and scheduler statistics.
    #[arg(long, env = "CADVISOR_RS_DISABLE_PROCESS_METRICS")]
    pub disable_process_metrics: bool,
    /// Disable TCP/UDP connection table statistics.
    #[arg(long, env = "CADVISOR_RS_DISABLE_NETWORK_CONNECTIONS")]
    pub disable_network_connections: bool,
    /// Disable statvfs capacity reads through container roots/mounts.
    #[arg(long, env = "CADVISOR_RS_DISABLE_FILESYSTEM_METRICS")]
    pub disable_filesystem_metrics: bool,
    /// Maximum processes and threads sampled per cgroup hierarchy.
    #[arg(long, env = "CADVISOR_RS_MAX_PROCESSES", default_value_t = 4096, value_parser = clap::value_parser!(u32).range(1..=1_000_000))]
    pub max_processes: u32,
    /// Collection interval in seconds.
    #[arg(long, env = "CADVISOR_RS_INTERVAL_SECONDS", default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..=3600))]
    pub interval_seconds: u64,
    /// Rediscovery and Docker metadata refresh interval in seconds.
    #[arg(long, env = "CADVISOR_RS_DISCOVERY_INTERVAL_SECONDS", default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..=3600))]
    pub discovery_interval_seconds: u64,
    /// Export only cgroups containing a Docker/containerd/CRI-O container ID.
    #[arg(long, env = "CADVISOR_RS_CONTAINERS_ONLY")]
    pub containers_only: bool,
    /// Maximum discovered cgroups; exceeding this limit fails the collection.
    #[arg(long, env = "CADVISOR_RS_MAX_CGROUPS", default_value_t = 10000, value_parser = clap::value_parser!(u32).range(1..=1_000_000))]
    pub max_cgroups: u32,
    /// Maximum discovery depth; exceeding this limit fails the collection.
    #[arg(long, env = "CADVISOR_RS_MAX_DEPTH", default_value_t = 64, value_parser = clap::value_parser!(u32).range(1..=256))]
    pub max_depth: u32,
    /// Docker label keys to export, e.g. --docker-label com.docker.compose.service.
    #[arg(long, env = "CADVISOR_RS_DOCKER_LABELS", value_delimiter = ',')]
    pub docker_label: Vec<String>,
    /// Print one Prometheus snapshot to stdout and exit without serving HTTP.
    #[arg(long)]
    pub once: bool,
}

impl Config {
    /// Sampling period; CLI validation guarantees it is nonzero.
    #[must_use]
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_seconds)
    }

    /// Maximum age accepted by the readiness endpoint.
    #[must_use]
    pub fn stale_after(&self) -> Duration {
        self.interval() * 3 + Duration::from_secs(5)
    }
}
