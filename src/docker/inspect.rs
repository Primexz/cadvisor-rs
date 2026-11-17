//! Minimal inspect DTO: do not retain environment variables, secrets or health logs.

use super::{Error, HealthSummary, fetch};
use crate::model::{ContainerState, DockerDetails, HealthStatus, Metadata};
use serde::Deserialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct State {
    status: Option<ContainerState>,
    running: Option<bool>,
    pid: Option<u32>,
    exit_code: Option<i64>,
    #[serde(rename = "OOMKilled")]
    oom_killed: Option<bool>,
    started_at: Option<String>,
    finished_at: Option<String>,
    health: Option<HealthSummary>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Host {
    memory: Option<u64>,
    memory_reservation: Option<u64>,
    memory_swap: Option<i64>,
    cpu_shares: Option<u64>,
    cpu_quota: Option<i64>,
    cpu_period: Option<u64>,
    nano_cpus: Option<u64>,
    pids_limit: Option<i64>,
    restart_policy: Option<RestartPolicy>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RestartPolicy {
    name: String,
    maximum_retry_count: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Mount {
    destination: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Config {
    healthcheck: Option<Healthcheck>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Healthcheck {
    #[serde(default)]
    test: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Inspect {
    state: Option<State>,
    restart_count: Option<u64>,
    host_config: Option<Host>,
    config: Option<Config>,
    #[serde(default)]
    mounts: Vec<Mount>,
}

pub(super) struct Inspection {
    details: DockerDetails,
    state: Option<ContainerState>,
    health: Option<HealthStatus>,
    streak: Option<u64>,
}

impl Inspection {
    pub(super) fn apply(self, metadata: &mut Metadata) {
        metadata.details = self.details;
        metadata.inspect_up = Some(true);
        if self.state.is_some() {
            metadata.state = self.state;
        }
        if self.health.is_some() {
            metadata.health = self.health;
            metadata.health_failing_streak = self.streak;
        }
    }
}

pub(super) async fn read(client: &reqwest::Client, id: &str) -> Result<Inspection, Error> {
    let value: Inspect = fetch(client, &format!("/containers/{id}/json"), 2 * 1024 * 1024).await?;
    let state = value
        .state
        .ok_or_else(|| std::io::Error::other("Docker inspect response has no State"))?;
    let host = value.host_config.unwrap_or_default();
    let configured = value.config.as_ref().map(|config| {
        config
            .healthcheck
            .as_ref()
            .is_some_and(|check| check.test.first().is_some_and(|kind| kind != "NONE"))
    });
    let health = if state.running == Some(false) {
        if state.health.is_some() || configured == Some(true) {
            Some(HealthStatus::Unknown)
        } else {
            configured.map(|_| HealthStatus::NoHealthcheck)
        }
    } else {
        state.health.as_ref().and_then(|h| h.status).or_else(|| {
            configured.map(|enabled| {
                if enabled {
                    HealthStatus::Unknown
                } else {
                    HealthStatus::NoHealthcheck
                }
            })
        })
    };
    let streak = state.health.as_ref().and_then(|h| h.failing_streak);
    let policy = host.restart_policy.as_ref().map(|p| match p.name.as_str() {
        "no" | "always" | "on-failure" | "unless-stopped" => p.name.clone(),
        _ => "unknown".into(),
    });
    let details = DockerDetails {
        pid: state.pid,
        started: state.started_at.as_deref().and_then(timestamp),
        finished: state.finished_at.as_deref().and_then(timestamp),
        running: state.running,
        healthcheck_configured: configured,
        exit_code: state.exit_code,
        oom_killed: state.oom_killed,
        restarts: value.restart_count,
        restart_policy: policy,
        restart_maximum: host.restart_policy.and_then(|p| p.maximum_retry_count),
        memory: host.memory,
        memory_reservation: host.memory_reservation,
        memory_swap: host.memory_swap,
        cpu_shares: host.cpu_shares,
        cpu_quota: host.cpu_quota,
        cpu_period: host.cpu_period,
        nano_cpus: host.nano_cpus,
        pids_limit: host.pids_limit,
        mounts: value
            .mounts
            .into_iter()
            .map(|m| m.destination)
            .filter(|path| {
                path.starts_with('/')
                    && !std::path::Path::new(path)
                        .components()
                        .any(|c| c == std::path::Component::ParentDir)
            })
            .collect(),
    };
    Ok(Inspection {
        details,
        state: state.status,
        health,
        streak,
    })
}

fn timestamp(value: &str) -> Option<f64> {
    OffsetDateTime::parse(value, &Rfc3339)
        .ok()
        .map(|date| date.unix_timestamp_nanos() as f64 / 1e9)
        .filter(|time| *time > 0.0)
}
