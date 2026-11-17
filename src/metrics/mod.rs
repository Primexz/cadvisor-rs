//! cAdvisor-compatible Prometheus text exposition, encoded once per collection.

mod controllers;
mod docker;
mod engine;
mod extended;
mod labels;
mod resources;

use std::fmt::Write;

use bytes::Bytes;

use crate::{collector::Collection, model::Container};

pub use labels::Labels;

/// Prometheus text exposition media type.
pub const CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

type Value = fn(&Container) -> Option<f64>;
type ScalarFamily = (&'static str, &'static str, &'static str, Value);

const SCALARS: &[ScalarFamily] = &[
    (
        "container_cpu_usage_seconds_total",
        "Cumulative CPU time in seconds.",
        "counter",
        |c| c.cpu.usage,
    ),
    (
        "container_cpu_user_seconds_total",
        "Cumulative user CPU time in seconds.",
        "counter",
        |c| c.cpu.user,
    ),
    (
        "container_cpu_system_seconds_total",
        "Cumulative system CPU time in seconds.",
        "counter",
        |c| c.cpu.system,
    ),
    (
        "container_cpu_cfs_periods_total",
        "Elapsed CFS periods.",
        "counter",
        |c| c.cpu.periods.map(|v| v as f64),
    ),
    (
        "container_cpu_cfs_throttled_periods_total",
        "Throttled CFS periods.",
        "counter",
        |c| c.cpu.throttled_periods.map(|v| v as f64),
    ),
    (
        "container_cpu_cfs_throttled_seconds_total",
        "Cumulative throttled time in seconds.",
        "counter",
        |c| c.cpu.throttled_seconds,
    ),
    (
        "container_spec_cpu_quota",
        "CPU quota in microseconds; -1 means unlimited.",
        "gauge",
        |c| c.cpu.quota.map(|v| v as f64),
    ),
    (
        "container_spec_cpu_period",
        "CFS period in microseconds.",
        "gauge",
        |c| c.cpu.period.map(|v| v as f64),
    ),
    (
        "container_memory_usage_bytes",
        "Current charged memory including cache in bytes.",
        "gauge",
        |c| c.memory.usage.map(|v| v as f64),
    ),
    (
        "container_memory_working_set_bytes",
        "Memory usage less inactive file pages in bytes.",
        "gauge",
        |c| c.memory.working_set.map(|v| v as f64),
    ),
    (
        "container_memory_rss",
        "Anonymous resident memory in bytes.",
        "gauge",
        |c| c.memory.rss.map(|v| v as f64),
    ),
    (
        "container_memory_cache",
        "File cache memory in bytes.",
        "gauge",
        |c| c.memory.cache.map(|v| v as f64),
    ),
    (
        "container_memory_swap",
        "Swap usage in bytes.",
        "gauge",
        |c| c.memory.swap.map(|v| v as f64),
    ),
    (
        "container_spec_memory_limit_bytes",
        "Effective memory limit in bytes.",
        "gauge",
        |c| c.memory.limit.map(|v| v as f64),
    ),
    (
        "container_memory_failcnt",
        "Memory limit hits.",
        "counter",
        |c| c.memory.failures.map(|v| v as f64),
    ),
    (
        "container_oom_events_total",
        "Cumulative OOM kills.",
        "counter",
        |c| c.memory.oom_kills.map(|v| v as f64),
    ),
    (
        "container_pids_current",
        "Current cgroup tasks, including threads.",
        "gauge",
        |c| c.pids.map(|v| v as f64),
    ),
    (
        "container_pids_limit",
        "Maximum cgroup tasks, if finite.",
        "gauge",
        |c| c.pids_limit.map(|v| v as f64),
    ),
    (
        "container_creation_time_seconds",
        "Container creation time since Unix epoch in seconds.",
        "gauge",
        |c| c.metadata.created.map(|v| v as f64),
    ),
];

fn family(out: &mut String, name: &str, help: &str, kind: &str) {
    let _ = writeln!(out, "# HELP {name} {help}\n# TYPE {name} {kind}");
}

fn gauge(out: &mut String, name: &str, help: &str, value: impl std::fmt::Display) {
    family(out, name, help, "gauge");
    let _ = writeln!(out, "{name} {value}");
}

/// Encode a snapshot in Prometheus text format with stable family and label order.
#[must_use]
pub fn encode(collection: &Collection, labels: &Labels) -> Bytes {
    let mut out = String::with_capacity(collection.containers.len().saturating_mul(3500));
    for (name, help, kind, value) in SCALARS {
        family(&mut out, name, help, kind);
        for container in &collection.containers {
            if let Some(value) = value(container) {
                out.push_str(name);
                let extra =
                    (*name == "container_cpu_usage_seconds_total").then_some(("cpu", "total"));
                labels.write(&mut out, container, extra);
                let _ = writeln!(out, " {value}");
            }
        }
    }
    extended::encode(&mut out, collection, labels);
    resources::encode(&mut out, collection, labels);
    controllers::encode(&mut out, collection, labels);
    docker::encode(&mut out, collection, labels);
    engine::encode(&mut out, collection);
    encode_devices(&mut out, collection, labels);
    for (name, help, value) in [
        (
            "machine_cpu_cores",
            "Logical CPU count.",
            collection.machine.cpu_cores,
        ),
        (
            "machine_memory_bytes",
            "Physical memory in bytes.",
            collection.machine.memory_bytes,
        ),
        (
            "machine_swap_bytes",
            "Swap capacity in bytes.",
            collection.machine.swap_bytes,
        ),
    ] {
        if let Some(value) = value {
            gauge(&mut out, name, help, value);
        }
    }
    gauge(
        &mut out,
        "container_scrape_error",
        "Whether core container collection encountered an error.",
        u8::from(collection.errors > 0),
    );
    gauge(
        &mut out,
        "machine_scrape_error",
        "Whether machine CPU or memory capacity was unavailable.",
        u8::from(
            collection.machine.cpu_cores.is_none() || collection.machine.memory_bytes.is_none(),
        ),
    );
    gauge(
        &mut out,
        "cadvisor_rs_collection_duration_seconds",
        "Filesystem collection duration in seconds.",
        collection.duration_seconds,
    );
    gauge(
        &mut out,
        "cadvisor_rs_collection_errors",
        "Unreadable or malformed controller files in the current collection.",
        collection.errors,
    );
    gauge(
        &mut out,
        "cadvisor_rs_collection_success",
        "Whether collection completed without controller read errors.",
        u8::from(collection.errors == 0),
    );
    gauge(
        &mut out,
        "cadvisor_rs_last_collection_timestamp_seconds",
        "Unix timestamp of the current snapshot.",
        collection.timestamp_seconds,
    );
    gauge(
        &mut out,
        "cadvisor_rs_containers",
        "Container/cgroup observations in the current snapshot, including Docker inventory.",
        collection.containers.len(),
    );
    gauge(
        &mut out,
        "cadvisor_rs_resource_cgroups",
        "Cgroups with live resource measurements.",
        collection
            .containers
            .iter()
            .filter(|c| c.has_resources())
            .count(),
    );
    gauge(
        &mut out,
        "cadvisor_rs_docker_inspect_errors",
        "Failed or deadline-skipped container inspect requests in the last refresh.",
        collection
            .containers
            .iter()
            .filter(|c| c.metadata.inspect_up == Some(false))
            .count(),
    );
    gauge(
        &mut out,
        "cadvisor_rs_docker_up",
        "Whether the last optional Docker metadata request succeeded.",
        u8::from(collection.docker_up),
    );
    family(
        &mut out,
        "cadvisor_rs_build_info",
        "Exporter build information.",
        "gauge",
    );
    let _ = writeln!(
        out,
        "cadvisor_rs_build_info{{version=\"{}\",cgroup_version=\"{}\"}} 1",
        env!("CARGO_PKG_VERSION"),
        collection.cgroup_version
    );
    Bytes::from(out)
}

fn encode_devices(out: &mut String, collection: &Collection, labels: &Labels) {
    type IoValue = fn(&crate::model::Io) -> Option<u64>;
    let io: &[(&str, IoValue)] = &[
        ("container_fs_reads_bytes_total", |i| i.read_bytes),
        ("container_fs_writes_bytes_total", |i| i.write_bytes),
        ("container_fs_reads_total", |i| i.reads),
        ("container_fs_writes_total", |i| i.writes),
    ];
    for (name, value) in io {
        family(out, name, "Cumulative block device I/O counter.", "counter");
        for container in &collection.containers {
            for (device, io) in &container.io {
                if let Some(value) = value(io) {
                    out.push_str(name);
                    labels.write(out, container, Some(("device", device)));
                    let _ = writeln!(out, " {value}");
                }
            }
        }
    }
    type NetworkValue = fn(&crate::model::Network) -> u64;
    let network: &[(&str, NetworkValue)] = &[
        ("container_network_receive_bytes_total", |n| n.rx_bytes),
        ("container_network_receive_packets_total", |n| n.rx_packets),
        ("container_network_receive_errors_total", |n| n.rx_errors),
        ("container_network_receive_packets_dropped_total", |n| {
            n.rx_dropped
        }),
        ("container_network_transmit_bytes_total", |n| n.tx_bytes),
        ("container_network_transmit_packets_total", |n| n.tx_packets),
        ("container_network_transmit_errors_total", |n| n.tx_errors),
        ("container_network_transmit_packets_dropped_total", |n| {
            n.tx_dropped
        }),
    ];
    for (name, value) in network {
        family(
            out,
            name,
            "Cumulative network interface counter.",
            "counter",
        );
        for container in &collection.containers {
            for (interface, network) in &container.network {
                out.push_str(name);
                labels.write(out, container, Some(("interface", interface)));
                let _ = writeln!(out, " {}", value(network));
            }
        }
    }
}

/// Minimal valid scrape emitted when no current collection could be obtained.
#[must_use]
pub fn failed() -> Bytes {
    Bytes::from_static(b"# HELP cadvisor_rs_collection_success Whether collection succeeded.\n# TYPE cadvisor_rs_collection_success gauge\ncadvisor_rs_collection_success 0\n# HELP container_scrape_error Whether core container collection failed.\n# TYPE container_scrape_error gauge\ncontainer_scrape_error 1\n")
}

fn sample(
    out: &mut String,
    name: &str,
    container: &Container,
    labels: &Labels,
    extra: &[(&str, &str)],
    value: impl std::fmt::Display,
) {
    out.push_str(name);
    labels.write_many(out, container, extra);
    let _ = writeln!(out, " {value}");
}

fn scalar_families(
    out: &mut String,
    collection: &Collection,
    labels: &Labels,
    descriptors: &[ScalarFamily],
) {
    for (name, help, kind, value) in descriptors {
        family(out, name, help, kind);
        for container in &collection.containers {
            if let Some(value) = value(container) {
                sample(out, name, container, labels, &[], value);
            }
        }
    }
}
