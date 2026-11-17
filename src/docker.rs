//! Optional Docker metadata through a persistent Unix-socket HTTP connection.

use std::{collections::BTreeMap, io, path::Path, time::Duration};

use serde::{Deserialize, de::DeserializeOwned};
use tokio::{
    task::JoinSet,
    time::{Instant, timeout_at},
};

use crate::model::{ContainerState, HealthStatus, Metadata};

mod engine;
mod inspect;

type Error = Box<dyn std::error::Error + Send + Sync>;

/// Bounded Docker collection options; inspect uses eight concurrent requests and a 5s budget.
#[derive(Clone, Copy, Debug)]
pub struct DockerOptions {
    /// Collect restart, start/exit, mount and configuration fields with inspect.
    pub inspect: bool,
    /// Exclude stopped containers from the inventory.
    pub running_only: bool,
    /// Request potentially expensive writable-layer sizes.
    pub sizes: bool,
    /// Maximum container inventory size.
    pub max_containers: u32,
}

impl Default for DockerOptions {
    fn default() -> Self {
        Self {
            inspect: true,
            running_only: false,
            sizes: false,
            max_containers: 10_000,
        }
    }
}

/// Full container IDs mapped to metadata; no Docker stats streams are opened.
pub type MetadataMap = BTreeMap<String, Metadata>;

/// A metadata client with a two-second deadline and an eight-MiB response limit.
pub struct DockerClient {
    client: reqwest::Client,
    labels: Vec<String>,
    options: DockerOptions,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Summary {
    id: String,
    #[serde(default)]
    names: Vec<String>,
    #[serde(default)]
    image: String,
    created: Option<u64>,
    state: Option<ContainerState>,
    #[serde(default)]
    status: String,
    health: Option<HealthSummary>,
    #[serde(default)]
    labels: Option<BTreeMap<String, String>>,
    #[serde(rename = "SizeRw")]
    writable_bytes: Option<i64>,
    #[serde(rename = "SizeRootFs")]
    rootfs_bytes: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct HealthSummary {
    status: Option<HealthStatus>,
    failing_streak: Option<u64>,
}

// Older Docker list responses omit structured Health. Only accept Docker's
// known terminal health markers; unexpected status text must not imply healthy.
fn legacy_health(status: &str) -> HealthStatus {
    if status.ends_with(" (healthy)") {
        HealthStatus::Healthy
    } else if status.ends_with(" (unhealthy)") {
        HealthStatus::Unhealthy
    } else if status.ends_with(" (health: starting)") {
        HealthStatus::Starting
    } else if status.starts_with("Up ") && !status.contains('(') {
        HealthStatus::NoHealthcheck
    } else {
        HealthStatus::Unknown
    }
}

impl DockerClient {
    /// Construct a Unix socket client without proxies, TLS or redirects.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new(socket: &Path, labels: Vec<String>) -> Result<Self, reqwest::Error> {
        Self::with_options(socket, labels, DockerOptions::default())
    }

    /// Construct a client with configurable inventory, inspect and layer-size collection.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn with_options(
        socket: &Path,
        labels: Vec<String>,
        options: DockerOptions,
    ) -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            .unix_socket(socket)
            .timeout(Duration::from_secs(2))
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            client,
            labels,
            options,
        })
    }

    /// Retrieve Docker inventory and selected inspect fields without stats streams.
    /// Inspect failures are per-container and do not invalidate a successful list.
    ///
    /// # Errors
    /// Returns an error on a transport failure, non-success response, oversized
    /// response or invalid JSON. Callers can retain their previous metadata.
    pub async fn refresh(&self) -> Result<MetadataMap, Error> {
        let mut path = format!("/containers/json?all={}", !self.options.running_only);
        if self.options.sizes {
            path.push_str("&size=true");
        }
        let summaries: Vec<Summary> = fetch(&self.client, &path, 8 * 1024 * 1024).await?;
        if summaries.len() > self.options.max_containers as usize {
            return Err(io::Error::other(
                "Docker inventory exceeds the configured container bound",
            )
            .into());
        }
        let mut metadata: MetadataMap = summaries
            .into_iter()
            .filter(|summary| {
                summary.id.len() == 64 && summary.id.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            .map(|summary| {
                let health = summary
                    .health
                    .as_ref()
                    .and_then(|health| health.status)
                    .unwrap_or_else(|| legacy_health(&summary.status));
                let health_failing_streak = match health {
                    HealthStatus::Healthy | HealthStatus::Unhealthy | HealthStatus::Starting => {
                        summary
                            .health
                            .as_ref()
                            .and_then(|health| health.failing_streak)
                    }
                    HealthStatus::NoHealthcheck | HealthStatus::Unknown => None,
                };
                let labels = summary
                    .labels
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|(key, _)| self.labels.contains(key))
                    .collect();
                (
                    summary.id.clone(),
                    Metadata {
                        runtime_id: summary.id,
                        name: summary
                            .names
                            .first()
                            .map(|name| name.trim_start_matches('/').to_owned())
                            .unwrap_or_default(),
                        image: summary.image,
                        created: summary.created,
                        health: Some(health),
                        health_failing_streak,
                        state: summary.state,
                        labels,
                        writable_bytes: summary.writable_bytes.and_then(|v| v.try_into().ok()),
                        rootfs_bytes: summary.rootfs_bytes.and_then(|v| v.try_into().ok()),
                        inspect_up: self.options.inspect.then_some(false),
                        ..Metadata::default()
                    },
                )
            })
            .collect();
        if self.options.inspect {
            self.enrich(&mut metadata).await;
        }
        Ok(metadata)
    }

    async fn enrich(&self, metadata: &mut MetadataMap) {
        let mut ids = metadata.keys().cloned().collect::<Vec<_>>().into_iter();
        let mut jobs = JoinSet::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            while jobs.len() < 8 {
                let Some(id) = ids.next() else {
                    break;
                };
                let client = self.client.clone();
                jobs.spawn(async move {
                    let result = inspect::read(&client, &id).await;
                    (id, result)
                });
            }
            if jobs.is_empty() {
                break;
            }
            match timeout_at(deadline, jobs.join_next()).await {
                Ok(Some(Ok((id, Ok(inspection))))) => {
                    if let Some(item) = metadata.get_mut(&id) {
                        inspection.apply(item);
                    }
                }
                Ok(Some(_)) => {}
                _ => {
                    jobs.abort_all();
                    break;
                }
            }
        }
    }

    /// Read selected Docker daemon /info fields.
    ///
    /// # Errors
    /// Returns an error if /info is inaccessible or malformed.
    pub async fn engine(&self) -> Result<crate::model::DockerEngine, Error> {
        engine::read(&self.client).await
    }
}

async fn fetch<T: DeserializeOwned>(
    client: &reqwest::Client,
    path: &str,
    maximum: usize,
) -> Result<T, Error> {
    let mut response = client
        .get(format!("http://localhost{path}"))
        .send()
        .await?
        .error_for_status()?;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len().saturating_add(chunk.len()) > maximum {
            return Err(io::Error::other(format!(
                "Docker metadata exceeds {} MiB",
                maximum / 1024 / 1024
            ))
            .into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&body)?)
}
