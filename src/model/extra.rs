//! Optional measurements with explicit availability and units.

use serde::Serialize;
use std::collections::BTreeMap;

/// Extended memory-controller gauges and counters. Every value is optional.
#[derive(Clone, Debug, Default, Serialize)]
pub struct MemoryDetails {
    /// Memory-controller NUMA residency in base pages, separated by hierarchy scope.
    pub numa: NumaStats,
    /// Whether the v1 cpuset migrates memory when its allowed nodes change.
    pub migrate: Option<bool>,
    /// Peak charged bytes since cgroup creation or kernel peak reset.
    pub peak: Option<u64>,
    /// File-backed mapped bytes.
    pub mapped_file: Option<u64>,
    /// Charged kernel bytes.
    pub kernel: Option<u64>,
    /// Active file bytes.
    pub active_file: Option<u64>,
    /// Inactive file bytes.
    pub inactive_file: Option<u64>,
    /// Dirty file bytes.
    pub dirty: Option<u64>,
    /// File bytes currently being written back.
    pub writeback: Option<u64>,
    /// Charged socket bytes.
    pub socket: Option<u64>,
    /// Charged slab bytes.
    pub slab: Option<u64>,
    /// Anonymous transparent hugepage bytes.
    pub anon_thp: Option<u64>,
    /// Pages scanned by reclaim.
    pub pgscan: Option<u64>,
    /// Pages reclaimed.
    pub pgsteal: Option<u64>,
    /// File working-set refaults.
    pub refault_file: Option<u64>,
    /// Anonymous working-set refaults.
    pub refault_anon: Option<u64>,
    /// Local minor/major fault counters; local minor means all page faults as in cAdvisor.
    pub faults: [Option<u64>; 2],
    /// Hierarchical page-fault and major-page-fault counters.
    pub hierarchical_faults: [Option<u64>; 2],
    /// Memory low events.
    pub events_low: Option<u64>,
    /// Memory high events.
    pub events_high: Option<u64>,
    /// Memory max events.
    pub events_max: Option<u64>,
    /// OOM allocation events (distinct from kills).
    pub events_oom: Option<u64>,
    /// Group OOM kills.
    pub events_oom_group: Option<u64>,
    /// Memory soft reservation/protection in bytes.
    pub reservation: Option<u64>,
    /// Resident plus swap limit in bytes, when finite.
    pub swap_limit: Option<u64>,
    /// Memory high throttle threshold in bytes, when finite.
    pub high: Option<u64>,
}

/// Anonymous, file-backed and unevictable base pages on one NUMA node.
#[derive(Clone, Debug, Default, Serialize)]
pub struct NumaNode {
    /// Anonymous pages.
    pub anon: Option<u64>,
    /// File-backed pages.
    pub file: Option<u64>,
    /// Unevictable pages; these can overlap anonymous or file-backed pages.
    pub unevictable: Option<u64>,
}

/// NUMA measurements with explicit local and descendant-inclusive accounting.
#[derive(Clone, Debug, Default, Serialize)]
pub struct NumaStats {
    /// Local v1 memory-controller statistics.
    pub container: BTreeMap<u32, NumaNode>,
    /// Descendant-inclusive v1 or unified v2 statistics.
    pub hierarchy: BTreeMap<u32, NumaNode>,
}

/// HugeTLB fault accounting for one hugepage size, independently of reservations.
#[derive(Clone, Debug, Default, Serialize)]
pub struct HugeTlb {
    /// Current charged bytes.
    pub usage: Option<u64>,
    /// Kernel-recorded peak bytes, where the v1 controller exposes them.
    pub peak: Option<u64>,
    /// Finite hugepage limit in bytes.
    pub limit: Option<u64>,
    /// Cumulative allocation failures due to the HugeTLB limit.
    pub failures: Option<u64>,
}

/// One PSI row: averages are percentages, total is cumulative seconds.
#[derive(Clone, Debug, Default, Serialize)]
pub struct PressureLine {
    /// Cumulative stall seconds.
    pub total: Option<f64>,
    /// Percentage averages for 10, 60 and 300 seconds.
    pub averages: [Option<f64>; 3],
}

/// Some-task and all-task pressure for one resource.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Pressure {
    /// At least one task was stalled.
    pub some: Option<PressureLine>,
    /// All non-idle tasks were stalled.
    pub full: Option<PressureLine>,
}

/// CPU, memory and I/O pressure, available on cgroup v2 with kernel PSI support.
#[derive(Clone, Debug, Default, Serialize)]
pub struct PressureSet {
    /// CPU pressure.
    pub cpu: Pressure,
    /// Memory pressure.
    pub memory: Pressure,
    /// I/O pressure.
    pub io: Pressure,
}

/// Soft and hard resource limits for the selected init process.
#[derive(Clone, Debug, Serialize)]
pub struct Ulimit {
    /// Normalized limit name, such as max_open_files.
    pub name: String,
    /// Soft limit; -1 means unlimited.
    pub soft: i64,
    /// Hard limit; -1 means unlimited.
    pub hard: i64,
}

/// Bounded process sampling. Incomplete aggregates are omitted, not partial sums.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Processes {
    /// Number of distinct process IDs in the sampled cgroup subtree.
    pub count: Option<u64>,
    /// Total number of threads.
    pub threads: Option<u64>,
    /// Open descriptors summed across processes.
    pub files: Option<u64>,
    /// Socket descriptors summed across processes.
    pub sockets: Option<u64>,
    /// Thread counts: running, sleeping, stopped, uninterruptible, zombie, idle, other.
    pub tasks: Option<[u64; 7]>,
    /// Scheduler runtime seconds summed across threads.
    pub sched_runtime: Option<f64>,
    /// Scheduler runqueue wait seconds summed across threads.
    pub sched_wait: Option<f64>,
    /// Scheduler dispatches summed across threads.
    pub sched_periods: Option<u64>,
    /// Limits for the Docker init process or first visible PID.
    pub ulimits: Vec<Ulimit>,
    /// Optional collection errors, including permission or configured bound failures.
    pub errors: u64,
}

/// TCP connection-state and UDP queue observations from one network namespace.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Connections {
    /// Namespace TCP SNMP and TcpExt measurements, including retransmits and drops.
    pub advanced: AdvancedTcp,
    /// TCP IPv4 counts in Linux state-code order (1..=11).
    pub tcp: Option<[u64; 11]>,
    /// TCP IPv6 counts.
    pub tcp6: Option<[u64; 11]>,
    /// UDP IPv4 listen count, drops, receive queue bytes and transmit queue bytes.
    pub udp: Option<[u64; 4]>,
    /// UDP IPv6 observations.
    pub udp6: Option<[u64; 4]>,
    /// Connection table collection errors.
    pub errors: u64,
}

/// A bounded set of named Linux TCP measurements; absent fields remain absent.
#[derive(Clone, Debug, Default, Serialize)]
pub struct AdvancedTcp {
    /// Known SNMP/TcpExt fields using cAdvisor's stable lowercase labels.
    pub values: BTreeMap<&'static str, u64>,
    /// Maximum connections; Linux reports -1 for a dynamic limit.
    pub max_connections: Option<i64>,
}

/// Filesystem capacity; capacity is shared with other containers using that filesystem.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Filesystem {
    /// Stable device label: rootfs or a container mount destination.
    pub device: String,
    /// Capacity in bytes.
    pub limit: Option<u64>,
    /// Writable-layer bytes from Docker (rootfs only, explicitly enabled).
    pub usage: Option<u64>,
    /// Available filesystem bytes.
    pub available: Option<u64>,
    /// Total inode capacity.
    pub inodes: Option<u64>,
    /// Free inodes.
    pub inodes_free: Option<u64>,
}

/// Selected Docker inspect fields. Secrets and application output are not retained.
#[derive(Clone, Debug, Default, Serialize)]
pub struct DockerDetails {
    /// Current init PID in the host namespace (zero for stopped containers).
    pub pid: Option<u32>,
    /// Most recent actual start timestamp in Unix seconds.
    pub started: Option<f64>,
    /// Most recent finish timestamp in Unix seconds.
    pub finished: Option<f64>,
    /// Current Docker restart-policy restart count; may reset on a manual start.
    pub restarts: Option<u64>,
    /// Last process exit code.
    pub exit_code: Option<i64>,
    /// Whether Docker marked the last termination as OOM killed.
    pub oom_killed: Option<bool>,
    /// Whether Docker reports the container running (paused containers are running).
    pub running: Option<bool>,
    /// Whether Docker's configuration defines an enabled HEALTHCHECK.
    pub healthcheck_configured: Option<bool>,
    /// Bounded Docker restart policy name.
    pub restart_policy: Option<String>,
    /// Maximum automatic restart attempts, if configured.
    pub restart_maximum: Option<u64>,
    /// Configured hard memory limit, zero for unlimited.
    pub memory: Option<u64>,
    /// Configured memory reservation.
    pub memory_reservation: Option<u64>,
    /// Configured resident plus swap limit; -1 means unlimited.
    pub memory_swap: Option<i64>,
    /// Configured proportional CPU shares.
    pub cpu_shares: Option<u64>,
    /// Configured CFS quota in microseconds.
    pub cpu_quota: Option<i64>,
    /// Configured CFS period in microseconds.
    pub cpu_period: Option<u64>,
    /// Configured CPU count in billionths of a core.
    pub nano_cpus: Option<u64>,
    /// Configured task limit; -1 means unlimited.
    pub pids_limit: Option<i64>,
    /// Container mount destinations; source host paths are not exported.
    pub mounts: Vec<String>,
}

/// Selected Docker /info capacity, inventory and daemon diagnostics.
#[derive(Clone, Debug, Default, Serialize)]
pub struct DockerEngine {
    /// Container counts keyed by running, paused and stopped.
    pub containers: BTreeMap<String, u64>,
    /// Number of images known to Docker.
    pub images: Option<u64>,
    /// Daemon file descriptors.
    pub files: Option<u64>,
    /// Daemon Go routines.
    pub goroutines: Option<u64>,
    /// Docker event listeners.
    pub event_listeners: Option<u64>,
    /// Logical CPUs visible to Docker.
    pub cpu_cores: Option<u64>,
    /// Memory visible to Docker, bytes.
    pub memory: Option<u64>,
    /// Server version.
    pub version: String,
    /// Storage driver.
    pub driver: String,
    /// Container operating system.
    pub os: String,
    /// Server architecture.
    pub arch: String,
    /// cgroup hierarchy version.
    pub cgroup_version: String,
    /// Docker cgroup driver.
    pub cgroup_driver: String,
}
