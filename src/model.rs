//! Typed measurements. Missing or unsupported kernel files remain absent.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod extra;
pub use extra::*;

/// Docker health-check state. No health check is distinct from a healthy check.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// The container has no configured Docker health check.
    #[serde(rename = "none")]
    NoHealthcheck,
    /// Docker is waiting for the health check to become healthy.
    Starting,
    /// The configured health check is passing.
    Healthy,
    /// The configured health check has failed its retry threshold.
    Unhealthy,
    /// Docker returned an unsupported or unavailable health state.
    #[serde(other)]
    Unknown,
}

impl HealthStatus {
    /// Bounded label value for Prometheus health series.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoHealthcheck => "none",
            Self::Starting => "starting",
            Self::Healthy => "healthy",
            Self::Unhealthy => "unhealthy",
            Self::Unknown => "unknown",
        }
    }
}

/// Docker lifecycle state for a container with a discovered cgroup.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerState {
    /// Created but not yet started.
    Created,
    /// Running normally.
    Running,
    /// Frozen by Docker pause.
    Paused,
    /// Waiting for a restart.
    Restarting,
    /// The main container process has exited.
    Exited,
    /// Docker is removing the container.
    Removing,
    /// Docker considers the container dead.
    Dead,
    /// An unsupported lifecycle state.
    #[serde(other)]
    Unknown,
}

impl ContainerState {
    /// Bounded label value for Prometheus lifecycle series.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Restarting => "restarting",
            Self::Exited => "exited",
            Self::Removing => "removing",
            Self::Dead => "dead",
            Self::Unknown => "unknown",
        }
    }
}

/// Cumulative CPU times in seconds, and CFS scheduler counters.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Cpu {
    /// Total user and system CPU time in seconds.
    pub usage: Option<f64>,
    /// User CPU time in seconds.
    pub user: Option<f64>,
    /// System CPU time in seconds.
    pub system: Option<f64>,
    /// Number of elapsed CFS periods.
    pub periods: Option<u64>,
    /// Number of throttled CFS periods.
    pub throttled_periods: Option<u64>,
    /// Cumulative time throttled, in seconds.
    pub throttled_seconds: Option<f64>,
    /// CFS quota in microseconds, or -1 for unlimited.
    pub quota: Option<i64>,
    /// CFS period in microseconds.
    pub period: Option<u64>,
    /// v1 proportional scheduler shares (or an explicit Docker CPU shares setting).
    pub shares: Option<u64>,
    /// Native v2 CPU weight, from 1 to 10000.
    pub weight: Option<u64>,
    /// Number of elapsed CFS burst periods.
    pub burst_periods: Option<u64>,
    /// Cumulative CFS burst CPU time in seconds.
    pub burst_seconds: Option<f64>,
    /// Number of CPUs in the effective cpuset, when available.
    pub cpuset_count: Option<u64>,
    /// Per-CPU cumulative usage in seconds where v1 cpuacct exposes it.
    pub per_cpu: Vec<f64>,
}

/// Current memory gauges in bytes and cumulative limit hits.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Memory {
    /// Total charged memory, including page cache.
    pub usage: Option<u64>,
    /// Usage less inactive file pages, clamped at zero.
    pub working_set: Option<u64>,
    /// Anonymous resident memory.
    pub rss: Option<u64>,
    /// File page cache.
    pub cache: Option<u64>,
    /// Swap use, excluding resident memory.
    pub swap: Option<u64>,
    /// Effective memory limit, capped at machine memory when known.
    pub limit: Option<u64>,
    /// Memory allocation failures or v2 max events.
    pub failures: Option<u64>,
    /// Cumulative OOM kills.
    pub oom_kills: Option<u64>,
    /// Additional byte gauges, page counters and limits from the memory controller.
    pub details: MemoryDetails,
}

/// Per-device block I/O counters; device names use the kernel major:minor format.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Io {
    /// Bytes read.
    pub read_bytes: Option<u64>,
    /// Bytes written.
    pub write_bytes: Option<u64>,
    /// Completed read operations.
    pub reads: Option<u64>,
    /// Completed write operations.
    pub writes: Option<u64>,
    /// Bytes discarded.
    pub discard_bytes: Option<u64>,
    /// Completed discard operations.
    pub discards: Option<u64>,
}

/// Per-interface network counters from a process's network namespace.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Network {
    /// Bytes received.
    pub rx_bytes: u64,
    /// Packets received.
    pub rx_packets: u64,
    /// Receive errors.
    pub rx_errors: u64,
    /// Receive drops.
    pub rx_dropped: u64,
    /// Bytes transmitted.
    pub tx_bytes: u64,
    /// Packets transmitted.
    pub tx_packets: u64,
    /// Transmit errors.
    pub tx_errors: u64,
    /// Transmit drops.
    pub tx_dropped: u64,
}

/// Optional Docker metadata. Cgroup collection never depends on Docker.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Metadata {
    /// Docker's full container ID, if this metadata came from Docker.
    pub runtime_id: String,
    /// First Docker name, with the leading slash removed.
    pub name: String,
    /// Image reference as reported by Docker.
    pub image: String,
    /// Container creation time in Unix seconds.
    pub created: Option<u64>,
    /// Docker HEALTHCHECK status, absent for cgroups without Docker metadata.
    pub health: Option<HealthStatus>,
    /// Consecutive failed health checks, when provided by the Docker list API.
    pub health_failing_streak: Option<u64>,
    /// Docker lifecycle state, when returned by the daemon.
    pub state: Option<ContainerState>,
    /// Whether the last requested inspect succeeded; absent if disabled.
    pub inspect_up: Option<bool>,
    /// Selected inspect fields; environment variables and health log output are not stored.
    pub details: DockerDetails,
    /// Writable-layer bytes, only requested with Docker size collection enabled.
    pub writable_bytes: Option<u64>,
    /// Image plus writable-layer bytes, when size collection is enabled.
    pub rootfs_bytes: Option<u64>,
    /// Explicitly allowlisted Docker labels.
    pub labels: BTreeMap<String, String>,
}

/// One cgroup observation with optional resource controllers.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Container {
    /// Absolute cgroup path relative to the configured hierarchy root.
    pub id: String,
    /// Best-effort metadata from the Docker daemon.
    pub metadata: Metadata,
    /// CPU measurements.
    pub cpu: Cpu,
    /// Memory measurements.
    pub memory: Memory,
    /// Current process/thread count reported by the pids controller.
    pub pids: Option<u64>,
    /// Process/thread limit; absent if unlimited.
    pub pids_limit: Option<u64>,
    /// Task creation failures due to the pids limit.
    pub pids_events_max: Option<u64>,
    /// Block I/O grouped by device.
    pub io: BTreeMap<String, Io>,
    /// Network counters grouped by interface, excluding loopback.
    pub network: BTreeMap<String, Network>,
    /// Cgroup v2 pressure stall information.
    pub pressure: PressureSet,
    /// Process, thread, descriptor and scheduler measurements.
    pub processes: Processes,
    /// Namespace TCP/UDP measurements.
    pub connections: Connections,
    /// Filesystem capacity and optional writable-layer use.
    pub filesystems: Vec<Filesystem>,
    /// Filesystem capacity reads that failed (commonly missing ptrace permission).
    pub filesystem_errors: u64,
    /// Per-container controller read errors for this sample.
    pub scrape_errors: u64,
}

impl Container {
    /// Whether live resource controller data exists, rather than Docker inventory alone.
    #[must_use]
    pub fn has_resources(&self) -> bool {
        self.cpu.usage.is_some()
            || self.memory.usage.is_some()
            || self.pids.is_some()
            || !self.io.is_empty()
    }
}

/// Host capacity parsed from the configured procfs.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Machine {
    /// Number of logical CPUs.
    pub cpu_cores: Option<u64>,
    /// Total physical memory in bytes.
    pub memory_bytes: Option<u64>,
    /// Total swap capacity in bytes.
    pub swap_bytes: Option<u64>,
}
