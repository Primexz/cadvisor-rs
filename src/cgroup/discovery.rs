//! Bounded, deterministic discovery that ignores child symbolic links.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use super::{Version, controller, parse::container_id};

/// A discovered cgroup with its stable hierarchy-relative identity.
#[derive(Clone, Debug)]
pub struct Group {
    /// Directory in the unified or selected v1 discovery controller.
    pub path: PathBuf,
    /// Path relative to the selected hierarchy, with a leading slash.
    pub id: String,
}

/// Enumerate the hierarchy without following child symlinks.
///
/// # Errors
/// Returns errors for unreadable directories, invalid UTF-8 names, unknown v1
/// layouts or exceeded limits. A vanished child during discovery is ignored.
pub fn discover(
    root: &Path,
    version: Version,
    containers_only: bool,
    max_count: u32,
    max_depth: u32,
) -> io::Result<Vec<Group>> {
    let base = match version {
        Version::V2 => root.to_path_buf(),
        Version::V1 => controller(
            root,
            &["memory", "cpuacct", "cpu,cpuacct", "cpuacct,cpu", "cpu"],
        ),
    };
    let mut pending = vec![(base.clone(), 0)];
    let mut groups = Vec::new();
    let mut visited = 0u32;
    while let Some((path, depth)) = pending.pop() {
        visited += 1;
        if visited > max_count {
            return Err(io::Error::other("cgroup count exceeds --max-cgroups"));
        }
        let relative = path.strip_prefix(&base).map_err(io::Error::other)?;
        let relative = relative
            .to_str()
            .ok_or_else(|| io::Error::other("non-UTF-8 cgroup name"))?;
        let id = format!("/{relative}");
        if !containers_only || container_id(&id).is_some() {
            groups.push(Group {
                path: path.clone(),
                id,
            });
        }
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(e) if depth > 0 && e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        for entry in entries {
            let entry = entry?;
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            if kind.is_dir() {
                if depth >= max_depth {
                    return Err(io::Error::other("cgroup depth exceeds --max-depth"));
                }
                pending.push((entry.path(), depth + 1));
            }
        }
    }
    groups.sort_unstable_by(|a, b| a.id.cmp(&b.id));
    Ok(groups)
}
