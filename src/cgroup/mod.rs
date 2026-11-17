//! Safe readers for the Linux cgroup v1 and v2 filesystem interfaces.

mod discovery;
mod memory;
mod parse;
mod v1;
mod v2;

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::model::Container;

pub use discovery::{Group, discover};
pub use parse::{container_id, cpuset_count, key_values, limit, parse_io_v2, parse_pressure};

/// Detected Linux cgroup hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    /// Separate controller mount points.
    V1,
    /// Unified hierarchy with cgroup.controllers at its root.
    V2,
}

impl Version {
    /// Detect the hierarchy and ensure its root is readable.
    ///
    /// # Errors
    /// Returns an I/O error if the root cannot be listed.
    pub fn detect(root: &Path) -> io::Result<Self> {
        fs::read_dir(root)?;
        Ok(if root.join("cgroup.controllers").exists() {
            Self::V2
        } else {
            Self::V1
        })
    }

    /// Human-readable version number for exporter diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "1",
            Self::V2 => "2",
        }
    }
}

/// Filesystem reader that counts unreadable or malformed controller data.
#[derive(Default)]
pub(crate) struct Reader {
    pub errors: u64,
}

impl Reader {
    pub fn text(&mut self, path: &Path) -> Option<String> {
        match fs::read_to_string(path) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(_) => {
                self.errors += 1;
                None
            }
        }
    }

    pub fn number(&mut self, path: &Path) -> Option<u64> {
        let text = self.text(path)?;
        let number = text.trim().parse().ok();
        if number.is_none() {
            self.errors += 1;
        }
        number
    }

    pub fn signed(&mut self, path: &Path) -> Option<i64> {
        let text = self.text(path)?;
        let number = text.trim().parse().ok();
        if number.is_none() {
            self.errors += 1;
        }
        number
    }
}

/// Read one cgroup. Absent controllers are omitted instead of reported as zero.
#[must_use]
pub fn read(
    root: &Path,
    version: Version,
    group: &Group,
    host_memory: Option<u64>,
) -> (Container, u64) {
    let mut reader = Reader::default();
    let mut container = match version {
        Version::V1 => v1::read(root, group, &mut reader),
        Version::V2 => v2::read(&group.path, &mut reader),
    };
    container.id.clone_from(&group.id);
    if let Some(host) = host_memory.filter(|_| container.memory.usage.is_some()) {
        container.memory.limit = Some(container.memory.limit.map_or(host, |limit| limit.min(host)));
    }
    (container, reader.errors)
}

pub(crate) fn controller(root: &Path, names: &[&str]) -> PathBuf {
    names
        .iter()
        .map(|name| root.join(name))
        .find(|path| path.is_dir())
        .unwrap_or_else(|| root.join(names[0]))
}
