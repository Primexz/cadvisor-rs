//! Docker daemon capacity and inventory from the existing local Unix API.

use super::{Error, fetch};
use crate::model::DockerEngine;
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Info {
    containers_running: Option<u64>,
    containers_paused: Option<u64>,
    containers_stopped: Option<u64>,
    images: Option<u64>,
    #[serde(rename = "NFd")]
    files: Option<u64>,
    #[serde(rename = "NGoroutines")]
    goroutines: Option<u64>,
    #[serde(rename = "NEventsListener")]
    event_listeners: Option<u64>,
    #[serde(rename = "NCPU")]
    cpu_cores: Option<u64>,
    mem_total: Option<u64>,
    #[serde(default)]
    server_version: String,
    #[serde(default)]
    driver: String,
    #[serde(rename = "OSType", default)]
    os: String,
    #[serde(default)]
    architecture: String,
    #[serde(default)]
    cgroup_version: String,
    #[serde(default)]
    cgroup_driver: String,
}

pub(super) async fn read(client: &reqwest::Client) -> Result<DockerEngine, Error> {
    let value: Info = fetch(client, "/info", 2 * 1024 * 1024).await?;
    let containers = [
        ("running", value.containers_running),
        ("paused", value.containers_paused),
        ("stopped", value.containers_stopped),
    ]
    .into_iter()
    .filter_map(|(state, value)| value.map(|v| (state.into(), v)))
    .collect();
    Ok(DockerEngine {
        containers,
        images: value.images,
        files: value.files,
        goroutines: value.goroutines,
        event_listeners: value.event_listeners,
        cpu_cores: value.cpu_cores,
        memory: value.mem_total,
        version: value.server_version,
        driver: value.driver,
        os: value.os,
        arch: value.architecture,
        cgroup_version: value.cgroup_version,
        cgroup_driver: value.cgroup_driver,
    })
}
