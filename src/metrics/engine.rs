//! Daemon inventory and capacity, separate from per-container resource metrics.

use super::{family, gauge, labels::pair};
use crate::collector::Collection;
use std::fmt::Write;

pub(super) fn encode(out: &mut String, collection: &Collection) {
    gauge(
        out,
        "docker_engine_up",
        "Whether the last Docker info request succeeded.",
        u8::from(collection.docker_engine.is_some()),
    );
    let Some(engine) = &collection.docker_engine else {
        return;
    };
    family(
        out,
        "docker_engine_containers",
        "Current Docker container counts by lifecycle state.",
        "gauge",
    );
    for (state, value) in &engine.containers {
        out.push_str("docker_engine_containers{");
        pair(out, "state", state);
        let _ = writeln!(out, "}} {value}");
    }
    for (name, help, value) in [
        (
            "docker_engine_images",
            "Images known to Docker.",
            engine.images,
        ),
        (
            "docker_engine_file_descriptors",
            "Docker daemon file descriptors.",
            engine.files,
        ),
        (
            "docker_engine_goroutines",
            "Docker daemon Go routines.",
            engine.goroutines,
        ),
        (
            "docker_engine_event_listeners",
            "Docker event listeners.",
            engine.event_listeners,
        ),
        (
            "docker_engine_cpu_cores",
            "Logical CPUs visible to Docker.",
            engine.cpu_cores,
        ),
        (
            "docker_engine_memory_bytes",
            "Memory visible to Docker in bytes.",
            engine.memory,
        ),
    ] {
        if let Some(value) = value {
            gauge(out, name, help, value);
        }
    }
    family(
        out,
        "docker_engine_info",
        "Docker version, platform and storage configuration.",
        "gauge",
    );
    out.push_str("docker_engine_info{");
    for (index, (name, value)) in [
        ("version", &engine.version),
        ("storage_driver", &engine.driver),
        ("os", &engine.os),
        ("arch", &engine.arch),
        ("cgroup_version", &engine.cgroup_version),
        ("cgroup_driver", &engine.cgroup_driver),
    ]
    .iter()
    .enumerate()
    {
        if index > 0 {
            out.push(',');
        }
        pair(out, name, value);
    }
    out.push_str("} 1\n");
}
