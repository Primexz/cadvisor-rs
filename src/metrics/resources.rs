//! Labelled memory faults, pressure, process, network and filesystem families.

use super::{Labels, family, sample};
use crate::{collector::Collection, model::Pressure};

pub(super) fn encode(out: &mut String, collection: &Collection, labels: &Labels) {
    family(
        out,
        "container_last_seen",
        "Last live cgroup observation in Unix seconds.",
        "gauge",
    );
    for c in collection.containers.iter().filter(|c| c.has_resources()) {
        sample(
            out,
            "container_last_seen",
            c,
            labels,
            &[],
            collection.timestamp_seconds,
        );
    }
    family(
        out,
        "container_memory_failures_total",
        "Cumulative page faults by failure type and hierarchy scope.",
        "counter",
    );
    for c in &collection.containers {
        for (scope, values) in [
            ("container", c.memory.details.faults),
            ("hierarchy", c.memory.details.hierarchical_faults),
        ] {
            for (i, kind) in ["pgfault", "pgmajfault"].iter().enumerate() {
                if let Some(value) = values[i] {
                    sample(
                        out,
                        "container_memory_failures_total",
                        c,
                        labels,
                        &[("failure_type", kind), ("scope", scope)],
                        value,
                    );
                }
            }
        }
    }
    family(
        out,
        "container_cpu_percpu_usage_seconds_total",
        "Cumulative CPU time per logical CPU where v1 cpuacct exposes it.",
        "counter",
    );
    for c in &collection.containers {
        for (i, value) in c.cpu.per_cpu.iter().enumerate() {
            sample(
                out,
                "container_cpu_percpu_usage_seconds_total",
                c,
                labels,
                &[("cpu", &format!("cpu{i:02}"))],
                value,
            );
        }
    }
    pressure(out, collection, labels);
    process(out, collection, labels);
    connections(out, collection, labels);
    filesystems(out, collection, labels);
    family(
        out,
        "container_blkio_device_usage_total",
        "Cumulative block I/O bytes by device and operation.",
        "counter",
    );
    for c in &collection.containers {
        for (device, io) in &c.io {
            let Some((major, minor)) = device.split_once(':') else {
                continue;
            };
            for (operation, value) in [
                ("Read", io.read_bytes),
                ("Write", io.write_bytes),
                ("Discard", io.discard_bytes),
            ] {
                if let Some(value) = value {
                    sample(
                        out,
                        "container_blkio_device_usage_total",
                        c,
                        labels,
                        &[
                            ("device", device),
                            ("major", major),
                            ("minor", minor),
                            ("operation", operation),
                        ],
                        value,
                    );
                }
            }
        }
    }
    for (name, bytes) in [
        ("container_fs_discards_bytes_total", true),
        ("container_fs_discards_total", false),
    ] {
        family(
            out,
            name,
            "Cumulative block device discard counter.",
            "counter",
        );
        for c in &collection.containers {
            for (device, io) in &c.io {
                if let Some(value) = if bytes { io.discard_bytes } else { io.discards } {
                    sample(out, name, c, labels, &[("device", device)], value);
                }
            }
        }
    }
}

fn pressure(out: &mut String, collection: &Collection, labels: &Labels) {
    type Get = fn(&crate::model::Container) -> &Pressure;
    let resources: &[(&str, Get)] = &[
        ("cpu", |c| &c.pressure.cpu),
        ("memory", |c| &c.pressure.memory),
        ("io", |c| &c.pressure.io),
    ];
    for (resource, get) in resources {
        for (kind, full) in [("waiting", false), ("stalled", true)] {
            let name = format!("container_pressure_{resource}_{kind}_seconds_total");
            family(
                out,
                &name,
                "Cumulative pressure stall time in seconds.",
                "counter",
            );
            for c in &collection.containers {
                let pressure = get(c);
                if let Some(value) = (if full { &pressure.full } else { &pressure.some })
                    .as_ref()
                    .and_then(|row| row.total)
                {
                    sample(out, &name, c, labels, &[], value);
                }
            }
            for (index, window) in [10, 60, 300].iter().enumerate() {
                let name = format!("container_pressure_{resource}_{kind}_avg{window}");
                family(out, &name, "Pressure stall percentage average.", "gauge");
                for c in &collection.containers {
                    let pressure = get(c);
                    if let Some(value) = (if full { &pressure.full } else { &pressure.some })
                        .as_ref()
                        .and_then(|row| row.averages[index])
                    {
                        sample(out, &name, c, labels, &[], value);
                    }
                }
            }
        }
    }
}

fn process(out: &mut String, collection: &Collection, labels: &Labels) {
    family(
        out,
        "container_tasks_state",
        "Container thread counts by scheduler state.",
        "gauge",
    );
    for c in &collection.containers {
        if let Some(values) = c.processes.tasks {
            for (i, state) in [
                "running",
                "sleeping",
                "stopped",
                "uninterruptible",
                "zombie",
                "idle",
                "other",
            ]
            .iter()
            .enumerate()
            {
                sample(
                    out,
                    "container_tasks_state",
                    c,
                    labels,
                    &[("state", state)],
                    values[i],
                );
            }
        }
    }
    for (name, hard) in [
        ("container_ulimits_soft", false),
        ("container_ulimits_hard", true),
    ] {
        family(
            out,
            name,
            "Init-process resource limits; -1 means unlimited.",
            "gauge",
        );
        for c in &collection.containers {
            for limit in &c.processes.ulimits {
                sample(
                    out,
                    name,
                    c,
                    labels,
                    &[("ulimit", &limit.name)],
                    if hard { limit.hard } else { limit.soft },
                );
            }
        }
    }
}

fn connections(out: &mut String, collection: &Collection, labels: &Labels) {
    for (name, v6) in [
        ("container_network_tcp_usage_total", false),
        ("container_network_tcp6_usage_total", true),
    ] {
        family(
            out,
            name,
            "Current TCP socket counts by state (a gauge, despite the legacy name).",
            "gauge",
        );
        for c in &collection.containers {
            if let Some(values) = if v6 {
                c.connections.tcp6
            } else {
                c.connections.tcp
            } {
                for (i, state) in [
                    "established",
                    "synsent",
                    "synrecv",
                    "finwait1",
                    "finwait2",
                    "timewait",
                    "close",
                    "closewait",
                    "lastack",
                    "listen",
                    "closing",
                ]
                .iter()
                .enumerate()
                {
                    sample(out, name, c, labels, &[("tcp_state", state)], values[i]);
                }
            }
        }
    }
    for (name, v6) in [
        ("container_network_udp_usage_total", false),
        ("container_network_udp6_usage_total", true),
    ] {
        family(
            out,
            name,
            "Current UDP socket, drop and queue observations (gauges).",
            "gauge",
        );
        for c in &collection.containers {
            if let Some(values) = if v6 {
                c.connections.udp6
            } else {
                c.connections.udp
            } {
                for (i, state) in ["listen", "dropped", "rxqueued", "txqueued"]
                    .iter()
                    .enumerate()
                {
                    sample(out, name, c, labels, &[("udp_state", state)], values[i]);
                }
            }
        }
    }
}

fn filesystems(out: &mut String, collection: &Collection, labels: &Labels) {
    type Get = fn(&crate::model::Filesystem) -> Option<u64>;
    let fields: &[(&str, Get)] = &[
        ("container_fs_limit_bytes", |f| f.limit),
        ("container_fs_usage_bytes", |f| f.usage),
        ("container_fs_available_bytes", |f| f.available),
        ("container_fs_inodes_total", |f| f.inodes),
        ("container_fs_inodes_free", |f| f.inodes_free),
    ];
    for (name, get) in fields {
        family(
            out,
            name,
            "Container filesystem capacity or writable-layer usage.",
            "gauge",
        );
        for c in &collection.containers {
            for fs in &c.filesystems {
                if let Some(value) = get(fs) {
                    sample(out, name, c, labels, &[("device", &fs.device)], value);
                }
            }
        }
    }
}
