//! Periodic collection with cached, bounded hierarchy discovery.

use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use crate::{
    cgroup::{self, Group, Version},
    config::Config,
    docker::MetadataMap,
    model::{Container, DockerEngine, Machine},
    procfs,
};

/// A complete sample. It can be encoded once and shared by all HTTP clients.
#[derive(Debug, Serialize)]
pub struct Collection {
    /// Cgroup hierarchy version.
    pub cgroup_version: String,
    /// Unix timestamp at collection start.
    pub timestamp_seconds: f64,
    /// Filesystem sampling time, excluding HTTP encoding and Docker refresh.
    pub duration_seconds: f64,
    /// Unreadable or malformed controller file count in this collection.
    pub errors: u64,
    /// True if the optional Docker metadata refresh most recently succeeded.
    pub docker_up: bool,
    /// Host resource capacity.
    pub machine: Machine,
    /// Observed cgroups, ordered by their stable cgroup path.
    pub containers: Vec<Container>,
    /// Optional Docker daemon capacity and inventory.
    pub docker_engine: Option<DockerEngine>,
}

/// Owns discovery state; filesystem access belongs on a blocking worker thread.
pub struct Collector {
    config: Config,
    version: Version,
    groups: Vec<Group>,
    discovered_at: Option<Instant>,
    identities: BTreeMap<String, String>,
    epoch: Instant,
}

impl Collector {
    /// Detect cgroup layout using the configured host mount.
    ///
    /// # Errors
    /// Returns an I/O error for an inaccessible root.
    pub fn new(config: Config) -> io::Result<Self> {
        let version = Version::detect(&config.cgroup_root)?;
        Ok(Self {
            config,
            version,
            groups: Vec::new(),
            discovered_at: None,
            identities: BTreeMap::new(),
            epoch: Instant::now(),
        })
    }

    /// Whether cgroup paths and optional Docker metadata should be refreshed.
    #[must_use]
    pub fn discovery_due(&self) -> bool {
        self.discovered_at.is_none_or(|at| {
            at.elapsed() >= Duration::from_secs(self.config.discovery_interval_seconds)
        })
    }

    /// Read resources once. Containers that exit between intervals are skipped.
    ///
    /// # Errors
    /// Returns an error if discovery fails, the mount disappears or no readable
    /// controller data exists in a nonempty selection.
    pub fn collect(&mut self, metadata: &MetadataMap, docker_up: bool) -> io::Result<Collection> {
        let start = Instant::now();
        std::fs::read_dir(&self.config.cgroup_root)?;
        if self.discovery_due() {
            self.groups = cgroup::discover(
                &self.config.cgroup_root,
                self.version,
                self.config.containers_only,
                self.config.max_cgroups,
                self.config.max_depth,
            )?;
            // Anchor deadlines before Docker/fs work so collection latency does
            // not postpone a 30s discovery to the next 40s sampling tick.
            let anchor = self.discovered_at.unwrap_or(self.epoch);
            let period = self.config.discovery_interval_seconds.max(1);
            let steps = anchor.elapsed().as_secs() / period;
            self.discovered_at = anchor.checked_add(Duration::from_secs(steps * period));
        }
        let timestamp_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let machine = procfs::machine(&self.config.proc_root);
        let mut containers = Vec::with_capacity(self.groups.len());
        let mut errors = 0;
        let mut seen = BTreeSet::new();
        for group in &self.groups {
            if !group.path.is_dir() {
                continue;
            }
            let (mut container, read_errors) = cgroup::read(
                &self.config.cgroup_root,
                self.version,
                group,
                machine.memory_bytes,
            );
            errors += read_errors;
            if !container.has_resources() {
                continue;
            }
            container.scrape_errors = read_errors;
            if let Some(info) = cgroup::container_id(&group.id).and_then(|id| metadata.get(id)) {
                container.metadata = info.clone();
                let id = cgroup::container_id(&group.id).unwrap_or_default();
                if container.metadata.runtime_id.is_empty() {
                    container.metadata.runtime_id = id.to_owned();
                }
                seen.insert(id.to_owned());
                self.identities
                    .entry(id.to_owned())
                    .and_modify(|path| {
                        if group.id.len() < path.len() {
                            path.clone_from(&group.id);
                        }
                    })
                    .or_insert_with(|| group.id.clone());
            } else {
                container.metadata.name.clone_from(&group.id);
            }
            container.network =
                procfs::network(&self.config.proc_root, &group.path, group.id == "/");
            configure(&mut container, machine.memory_bytes);
            let prefix = if group.id == "/" {
                "/".to_owned()
            } else {
                format!("{}/", group.id)
            };
            let first = self
                .groups
                .partition_point(|candidate| candidate.id < prefix);
            let paths: Vec<_> = std::iter::once(group.path.clone())
                .chain(
                    self.groups[first..]
                        .iter()
                        .take_while(|candidate| candidate.id.starts_with(&prefix))
                        .filter(|candidate| candidate.id != group.id)
                        .map(|candidate| candidate.path.clone()),
                )
                .collect();
            let ids = procfs::pids(&paths, self.config.max_processes);
            let ids = match ids {
                Ok(ids) => ids,
                Err(_) => {
                    if !self.config.disable_process_metrics {
                        container.processes.errors += 1;
                    }
                    Vec::new()
                }
            };
            let init = container
                .metadata
                .details
                .pid
                .filter(|pid| ids.contains(pid))
                .or_else(|| ids.first().copied());
            if !self.config.disable_process_metrics && container.processes.errors == 0 {
                container.processes = procfs::read_processes(
                    &self.config.proc_root,
                    &ids,
                    init,
                    self.config.max_processes,
                );
            }
            if !self.config.disable_network_connections {
                container.connections =
                    procfs::read_connections(&self.config.proc_root, init, group.id == "/");
            }
            if !self.config.disable_filesystem_metrics {
                (container.filesystems, container.filesystem_errors) = procfs::read_filesystems(
                    &self.config.proc_root,
                    init,
                    &container.metadata.details.mounts,
                    container.metadata.writable_bytes,
                );
            }
            containers.push(container);
        }
        if containers.is_empty() && !self.config.containers_only {
            return Err(io::Error::other(
                "no readable cgroup controller data; check host cgroup mounts",
            ));
        }
        // Keep stopped/created containers visible as inventory, without inventing
        // zero CPU/memory/network measurements for nonexistent cgroups.
        for (id, info) in metadata.iter().filter(|(id, _)| !seen.contains(*id)) {
            let mut container = Container {
                id: self
                    .identities
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| format!("/docker/{id}")),
                metadata: info.clone(),
                ..Container::default()
            };
            if container.metadata.runtime_id.is_empty() {
                container.metadata.runtime_id = id.clone();
            }
            configure(&mut container, machine.memory_bytes);
            (container.filesystems, container.filesystem_errors) =
                procfs::read_filesystems(&self.config.proc_root, None, &[], info.writable_bytes);
            containers.push(container);
        }
        self.identities.retain(|id, _| metadata.contains_key(id));
        containers.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        Ok(Collection {
            cgroup_version: self.version.as_str().to_owned(),
            timestamp_seconds,
            duration_seconds: start.elapsed().as_secs_f64(),
            errors,
            docker_up,
            machine,
            containers,
            docker_engine: None,
        })
    }
}

fn configure(container: &mut Container, host_memory: Option<u64>) {
    let details = &container.metadata.details;
    container.cpu.shares = container
        .cpu
        .shares
        .or(details.cpu_shares.filter(|v| *v > 0));
    container.cpu.quota = container
        .cpu
        .quota
        .or(details.cpu_quota.map(|v| if v == 0 { -1 } else { v }));
    container.cpu.period = container
        .cpu
        .period
        .or(details.cpu_period.filter(|v| *v > 0));
    container.memory.limit = container.memory.limit.or_else(|| {
        details.memory.and_then(|v| {
            if v == 0 {
                host_memory
            } else {
                Some(host_memory.map_or(v, |host| host.min(v)))
            }
        })
    });
    container.memory.details.reservation = container
        .memory
        .details
        .reservation
        .or(details.memory_reservation);
    container.memory.details.swap_limit = container.memory.details.swap_limit.or(details
        .memory_swap
        .filter(|v| *v > 0)
        .and_then(|v| v.try_into().ok()));
    container.pids_limit = container.pids_limit.or(details
        .pids_limit
        .and_then(|v| v.try_into().ok())
        .filter(|v| *v > 0));
}
