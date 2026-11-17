//! Docker health and lifecycle gauges from the same cached metadata-list request.

use std::fmt::Write;

use crate::collector::Collection;

use super::{Labels, family, sample};

pub(super) fn encode(out: &mut String, collection: &Collection, labels: &Labels) {
    // Identity can remain useful after a metadata refresh failure, but cached
    // health/state must not continue to report a healthy current observation.
    if !collection.docker_up {
        return;
    }
    scalar_metrics(out, collection, labels);
    family(
        out,
        "container_health_status",
        "Docker health check status; value 1 denotes the current status label.",
        "gauge",
    );
    for container in &collection.containers {
        if let Some(health) = container.metadata.health {
            out.push_str("container_health_status");
            labels.write(out, container, Some(("status", health.as_str())));
            out.push_str(" 1\n");
        }
    }
    family(
        out,
        "container_health_state",
        "Health check result: 1 passing, 0 not passing, -1 no check.",
        "gauge",
    );
    for c in &collection.containers {
        if let Some(health) = c.metadata.health {
            let value = match health {
                crate::model::HealthStatus::Healthy => 1,
                crate::model::HealthStatus::NoHealthcheck => -1,
                _ => 0,
            };
            sample(out, "container_health_state", c, labels, &[], value);
        }
    }
    family(
        out,
        "container_health_failing_streak",
        "Consecutive failed Docker health checks, when available.",
        "gauge",
    );
    for container in &collection.containers {
        if let Some(streak) = container.metadata.health_failing_streak {
            out.push_str("container_health_failing_streak");
            labels.write(out, container, None);
            let _ = writeln!(out, " {streak}");
        }
    }
    family(
        out,
        "container_state",
        "Docker lifecycle state; value 1 denotes the current state label.",
        "gauge",
    );
    for container in &collection.containers {
        if let Some(state) = container.metadata.state {
            out.push_str("container_state");
            labels.write(out, container, Some(("state", state.as_str())));
            out.push_str(" 1\n");
        }
    }
    family(
        out,
        "container_info",
        "Docker container identity and image information.",
        "gauge",
    );
    // Keep each family's samples contiguous for Prometheus parser compatibility.
    for c in &collection.containers {
        if !c.metadata.runtime_id.is_empty() {
            sample(
                out,
                "container_info",
                c,
                labels,
                &[("container_id", &c.metadata.runtime_id)],
                1,
            );
        }
    }
    family(
        out,
        "container_restart_policy",
        "Docker restart policy configuration.",
        "gauge",
    );
    for c in &collection.containers {
        if let Some(policy) = &c.metadata.details.restart_policy {
            sample(
                out,
                "container_restart_policy",
                c,
                labels,
                &[("policy", policy)],
                1,
            );
        }
    }
}

fn scalar_metrics(out: &mut String, collection: &Collection, labels: &Labels) {
    use crate::model::{Container, ContainerState};
    type Get = fn(&Container) -> Option<f64>;
    let fields: &[(&str, &str, &str, Get)] = &[
        (
            "container_healthcheck_configured",
            "Whether an enabled Docker HEALTHCHECK is configured.",
            "gauge",
            |c| {
                c.metadata
                    .details
                    .healthcheck_configured
                    .map(|v| f64::from(u8::from(v)))
            },
        ),
        (
            "container_running",
            "Whether Docker reports the container running (including paused).",
            "gauge",
            |c| {
                c.metadata
                    .details
                    .running
                    .or_else(|| {
                        c.metadata.state.and_then(|s| match s {
                            ContainerState::Unknown => None,
                            ContainerState::Running
                            | ContainerState::Paused
                            | ContainerState::Restarting => Some(true),
                            _ => Some(false),
                        })
                    })
                    .map(|v| f64::from(u8::from(v)))
            },
        ),
        (
            "container_restarts_total",
            "Docker restart-policy count; may reset on manual start.",
            "counter",
            |c| c.metadata.details.restarts.map(|v| v as f64),
        ),
        (
            "container_start_time_seconds",
            "Most recent actual Docker start time in Unix seconds.",
            "gauge",
            |c| c.metadata.details.started,
        ),
        (
            "container_finished_time_seconds",
            "Most recent actual Docker finish time in Unix seconds.",
            "gauge",
            |c| c.metadata.details.finished,
        ),
        (
            "container_last_exit_code",
            "Docker-reported last exit code.",
            "gauge",
            |c| c.metadata.details.exit_code.map(|v| v as f64),
        ),
        (
            "container_oom_killed",
            "Whether Docker marked the last termination OOM killed.",
            "gauge",
            |c| {
                c.metadata
                    .details
                    .oom_killed
                    .map(|v| f64::from(u8::from(v)))
            },
        ),
        (
            "container_pid",
            "Docker init process host PID, zero for stopped containers.",
            "gauge",
            |c| c.metadata.details.pid.map(f64::from),
        ),
        (
            "container_docker_inspect_up",
            "Whether the last requested Docker inspect succeeded.",
            "gauge",
            |c| c.metadata.inspect_up.map(|v| f64::from(u8::from(v))),
        ),
        (
            "container_restart_maximum_retry_count",
            "Configured Docker automatic restart attempt limit.",
            "gauge",
            |c| c.metadata.details.restart_maximum.map(|v| v as f64),
        ),
        (
            "container_fs_rootfs_size_bytes",
            "Docker image plus writable-layer bytes; size collection must be enabled.",
            "gauge",
            |c| c.metadata.rootfs_bytes.map(|v| v as f64),
        ),
    ];
    super::scalar_families(out, collection, labels, fields);
    family(
        out,
        "container_uptime_seconds",
        "Elapsed seconds since the actual start of a currently running Docker container.",
        "gauge",
    );
    for c in &collection.containers {
        if c.metadata.details.running == Some(true)
            && let Some(started) = c.metadata.details.started
        {
            sample(
                out,
                "container_uptime_seconds",
                c,
                labels,
                &[],
                (collection.timestamp_seconds - started).max(0.0),
            );
        }
    }
}
