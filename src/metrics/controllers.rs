//! Optional HugeTLB, NUMA and detailed block controller measurements.

use super::{Labels, family, sample};
use crate::{
    collector::Collection,
    model::{HugeTlb, Io},
};

pub(super) fn encode(out: &mut String, collection: &Collection, labels: &Labels) {
    hugetlb(out, collection, labels);
    numa(out, collection, labels);
    io(out, collection, labels);
}

fn hugetlb(out: &mut String, collection: &Collection, labels: &Labels) {
    type Get = fn(&HugeTlb) -> Option<u64>;
    let fields: &[(&str, &str, &str, Get)] = &[
        (
            "container_hugetlb_usage_bytes",
            "Current charged HugeTLB bytes.",
            "gauge",
            |value| value.usage,
        ),
        (
            "container_hugetlb_max_usage_bytes",
            "Kernel-recorded peak HugeTLB bytes.",
            "gauge",
            |value| value.peak,
        ),
        (
            "container_hugetlb_failcnt",
            "Cumulative allocation failures due to HugeTLB limits.",
            "counter",
            |value| value.failures,
        ),
        (
            "container_spec_hugetlb_limit_bytes",
            "Finite HugeTLB limit in bytes.",
            "gauge",
            |value| value.limit,
        ),
    ];
    for (name, help, kind, get) in fields {
        family(out, name, help, kind);
        for container in &collection.containers {
            for (size, value) in &container.hugetlb {
                if let Some(value) = get(value) {
                    sample(out, name, container, labels, &[("pagesize", size)], value);
                }
            }
        }
    }
}

fn numa(out: &mut String, collection: &Collection, labels: &Labels) {
    family(
        out,
        "container_memory_numa_pages",
        "Resident base pages by NUMA node, memory type and hierarchy scope.",
        "gauge",
    );
    for container in &collection.containers {
        let numa = &container.memory.details.numa;
        for (scope, nodes) in [
            ("container", &numa.container),
            ("hierarchy", &numa.hierarchy),
        ] {
            for (node, values) in nodes {
                let node = node.to_string();
                for (kind, value) in [
                    ("anon", values.anon),
                    ("file", values.file),
                    ("unevictable", values.unevictable),
                ] {
                    if let Some(value) = value {
                        sample(
                            out,
                            "container_memory_numa_pages",
                            container,
                            labels,
                            &[("type", kind), ("scope", scope), ("node", &node)],
                            value,
                        );
                    }
                }
            }
        }
    }
    family(
        out,
        "container_memory_migrate",
        "Whether v1 cpuset memory migration is enabled.",
        "gauge",
    );
    for container in &collection.containers {
        if let Some(value) = container.memory.details.migrate {
            sample(
                out,
                "container_memory_migrate",
                container,
                labels,
                &[],
                u8::from(value),
            );
        }
    }
}

fn io(out: &mut String, collection: &Collection, labels: &Labels) {
    type Count = fn(&Io) -> Option<u64>;
    let counts: &[(&str, &str, &str, Count)] = &[
        (
            "container_fs_reads_merged_total",
            "Read requests merged by the block scheduler.",
            "counter",
            |value| value.reads_merged,
        ),
        (
            "container_fs_writes_merged_total",
            "Write requests merged by the block scheduler.",
            "counter",
            |value| value.writes_merged,
        ),
        (
            "container_fs_io_current",
            "Current I/O requests queued by the block scheduler.",
            "gauge",
            |value| value.io_current,
        ),
    ];
    for (name, help, kind, get) in counts {
        family(out, name, help, kind);
        for container in &collection.containers {
            for (device, value) in &container.io {
                if let Some(value) = get(value) {
                    sample(out, name, container, labels, &[("device", device)], value);
                }
            }
        }
    }
    type Time = fn(&Io) -> Option<f64>;
    let times: &[(&str, &str, Time)] = &[
        (
            "container_fs_read_seconds_total",
            "Cumulative read request service time in seconds.",
            |value| value.read_seconds,
        ),
        (
            "container_fs_write_seconds_total",
            "Cumulative write request service time in seconds.",
            |value| value.write_seconds,
        ),
        (
            "container_fs_io_time_seconds_total",
            "Cumulative I/O request service time in seconds.",
            |value| value.service_seconds,
        ),
        (
            "container_fs_io_cost_usage_seconds_total",
            "Cumulative IOCost usage in seconds.",
            |value| value.cost_usage_seconds,
        ),
        (
            "container_fs_io_cost_wait_seconds_total",
            "Cumulative IOCost wait time in seconds.",
            |value| value.cost_wait_seconds,
        ),
        (
            "container_fs_io_cost_indebt_seconds_total",
            "Cumulative IOCost time in debt in seconds.",
            |value| value.cost_indebt_seconds,
        ),
        (
            "container_fs_io_cost_indelay_seconds_total",
            "Cumulative IOCost delay time in seconds.",
            |value| value.cost_indelay_seconds,
        ),
    ];
    for (name, help, get) in times {
        family(out, name, help, "counter");
        for container in &collection.containers {
            for (device, value) in &container.io {
                if let Some(value) = get(value) {
                    sample(out, name, container, labels, &[("device", device)], value);
                }
            }
        }
    }
}
