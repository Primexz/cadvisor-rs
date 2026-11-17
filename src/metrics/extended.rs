//! Extended CPU, memory, task and scheduler families using cAdvisor names.

use super::{Labels, ScalarFamily, scalar_families};
use crate::collector::Collection;

const FAMILIES: &[ScalarFamily] = &[
    (
        "container_filesystem_scrape_error",
        "Whether optional filesystem capacity sampling failed.",
        "gauge",
        |c| {
            c.has_resources()
                .then_some(f64::from(u8::from(c.filesystem_errors > 0)))
        },
    ),
    (
        "container_cpu_cfs_burst_periods_total",
        "Number of CFS burst periods.",
        "counter",
        |c| c.cpu.burst_periods.map(|v| v as f64),
    ),
    (
        "container_cpu_cfs_burst_seconds_total",
        "Cumulative CFS burst time in seconds.",
        "counter",
        |c| c.cpu.burst_seconds,
    ),
    (
        "container_spec_cpu_shares",
        "Proportional scheduler shares.",
        "gauge",
        |c| c.cpu.shares.map(|v| v as f64),
    ),
    (
        "container_spec_cpu_weight",
        "Native cgroup v2 CPU weight.",
        "gauge",
        |c| c.cpu.weight.map(|v| v as f64),
    ),
    (
        "container_spec_cpu_cpuset_count",
        "CPUs in the effective cpuset.",
        "gauge",
        |c| c.cpu.cpuset_count.map(|v| v as f64),
    ),
    (
        "container_spec_cpu_limit_cores",
        "Configured CPU quota in logical cores.",
        "gauge",
        |c| {
            c.cpu
                .quota
                .zip(c.cpu.period)
                .filter(|(q, p)| *q > 0 && *p > 0)
                .map(|(q, p)| q as f64 / p as f64)
                .or_else(|| {
                    c.metadata
                        .details
                        .nano_cpus
                        .filter(|v| *v > 0)
                        .map(|v| v as f64 / 1e9)
                })
        },
    ),
    (
        "container_memory_max_usage_bytes",
        "Peak charged memory bytes.",
        "gauge",
        |c| c.memory.details.peak.map(|v| v as f64),
    ),
    (
        "container_memory_kernel_usage",
        "Charged kernel memory bytes.",
        "gauge",
        |c| c.memory.details.kernel.map(|v| v as f64),
    ),
    (
        "container_memory_mapped_file",
        "Mapped file memory bytes.",
        "gauge",
        |c| c.memory.details.mapped_file.map(|v| v as f64),
    ),
    (
        "container_memory_total_active_file_bytes",
        "Active file memory bytes.",
        "gauge",
        |c| c.memory.details.active_file.map(|v| v as f64),
    ),
    (
        "container_memory_total_inactive_file_bytes",
        "Inactive file memory bytes.",
        "gauge",
        |c| c.memory.details.inactive_file.map(|v| v as f64),
    ),
    (
        "container_memory_file_dirty_bytes",
        "Dirty file memory bytes.",
        "gauge",
        |c| c.memory.details.dirty.map(|v| v as f64),
    ),
    (
        "container_memory_file_writeback_bytes",
        "File writeback memory bytes.",
        "gauge",
        |c| c.memory.details.writeback.map(|v| v as f64),
    ),
    (
        "container_memory_socket_usage_bytes",
        "Socket buffer memory bytes.",
        "gauge",
        |c| c.memory.details.socket.map(|v| v as f64),
    ),
    (
        "container_memory_slab_bytes",
        "Kernel slab memory bytes.",
        "gauge",
        |c| c.memory.details.slab.map(|v| v as f64),
    ),
    (
        "container_memory_anon_thp_bytes",
        "Anonymous transparent hugepage bytes.",
        "gauge",
        |c| c.memory.details.anon_thp.map(|v| v as f64),
    ),
    (
        "container_memory_pgscan_total",
        "Pages scanned by memory reclaim.",
        "counter",
        |c| c.memory.details.pgscan.map(|v| v as f64),
    ),
    (
        "container_memory_pgsteal_total",
        "Pages reclaimed.",
        "counter",
        |c| c.memory.details.pgsteal.map(|v| v as f64),
    ),
    (
        "container_memory_workingset_refault_file_total",
        "File working-set refaults.",
        "counter",
        |c| c.memory.details.refault_file.map(|v| v as f64),
    ),
    (
        "container_memory_workingset_refault_anon_total",
        "Anonymous working-set refaults.",
        "counter",
        |c| c.memory.details.refault_anon.map(|v| v as f64),
    ),
    (
        "container_memory_events_low_total",
        "Memory low events.",
        "counter",
        |c| c.memory.details.events_low.map(|v| v as f64),
    ),
    (
        "container_memory_events_high_total",
        "Memory high events.",
        "counter",
        |c| c.memory.details.events_high.map(|v| v as f64),
    ),
    (
        "container_memory_events_max_total",
        "Memory max events.",
        "counter",
        |c| c.memory.details.events_max.map(|v| v as f64),
    ),
    (
        "container_memory_events_oom_total",
        "OOM allocation events.",
        "counter",
        |c| c.memory.details.events_oom.map(|v| v as f64),
    ),
    (
        "container_memory_events_oom_group_kill_total",
        "Group OOM kills.",
        "counter",
        |c| c.memory.details.events_oom_group.map(|v| v as f64),
    ),
    (
        "container_spec_memory_reservation_limit_bytes",
        "Memory reservation or protection in bytes.",
        "gauge",
        |c| c.memory.details.reservation.map(|v| v as f64),
    ),
    (
        "container_spec_memory_swap_limit_bytes",
        "Configured resident plus swap limit bytes.",
        "gauge",
        |c| c.memory.details.swap_limit.map(|v| v as f64),
    ),
    (
        "container_spec_memory_high_bytes",
        "Finite memory high throttle threshold bytes.",
        "gauge",
        |c| c.memory.details.high.map(|v| v as f64),
    ),
    (
        "container_pids_events_max_total",
        "Task creation failures due to pids limits.",
        "counter",
        |c| c.pids_events_max.map(|v| v as f64),
    ),
    (
        "container_processes",
        "Processes in the cgroup subtree.",
        "gauge",
        |c| c.processes.count.map(|v| v as f64),
    ),
    (
        "container_threads",
        "Threads in the cgroup subtree.",
        "gauge",
        |c| c.pids.or(c.processes.threads).map(|v| v as f64),
    ),
    (
        "container_threads_max",
        "Finite maximum cgroup threads.",
        "gauge",
        |c| c.pids_limit.map(|v| v as f64),
    ),
    (
        "container_file_descriptors",
        "Open process file descriptors.",
        "gauge",
        |c| c.processes.files.map(|v| v as f64),
    ),
    (
        "container_sockets",
        "Open process socket descriptors.",
        "gauge",
        |c| c.processes.sockets.map(|v| v as f64),
    ),
    (
        "container_cpu_schedstat_run_seconds_total",
        "Scheduler runtime across container threads in seconds.",
        "counter",
        |c| c.processes.sched_runtime,
    ),
    (
        "container_cpu_schedstat_runqueue_seconds_total",
        "Scheduler queue wait across threads in seconds.",
        "counter",
        |c| c.processes.sched_wait,
    ),
    (
        "container_cpu_schedstat_run_periods_total",
        "Scheduler dispatch periods across threads.",
        "counter",
        |c| c.processes.sched_periods.map(|v| v as f64),
    ),
    (
        "container_process_scrape_error",
        "Whether optional process sampling was incomplete.",
        "gauge",
        |c| {
            c.has_resources()
                .then_some(f64::from(u8::from(c.processes.errors > 0)))
        },
    ),
    (
        "container_network_scrape_error",
        "Whether optional socket-table sampling was incomplete.",
        "gauge",
        |c| {
            c.has_resources()
                .then_some(f64::from(u8::from(c.connections.errors > 0)))
        },
    ),
    (
        "container_resource_scrape_error",
        "Whether this cgroup had controller read errors.",
        "gauge",
        |c| {
            c.has_resources()
                .then_some(f64::from(u8::from(c.scrape_errors > 0)))
        },
    ),
    (
        "container_resource_available",
        "Whether live cgroup resource data are present.",
        "gauge",
        |c| Some(f64::from(u8::from(c.has_resources()))),
    ),
];

pub(super) fn encode(out: &mut String, collection: &Collection, labels: &Labels) {
    scalar_families(out, collection, labels, FAMILIES);
}
