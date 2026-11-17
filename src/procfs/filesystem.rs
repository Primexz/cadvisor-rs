//! Safe statvfs capacity reads through a container's procfs root.

use crate::model::Filesystem;
use std::{collections::BTreeSet, path::Path};

/// Sample root and mount filesystem capacities. Writable usage comes from Docker,
/// because statvfs would report shared host filesystem usage, not container usage.
#[must_use]
pub fn read_filesystems(
    proc_root: &Path,
    pid: Option<u32>,
    mounts: &[String],
    writable: Option<u64>,
) -> (Vec<Filesystem>, u64) {
    let Some(pid) = pid.filter(|pid| *pid > 0) else {
        return (
            writable
                .map(|usage| Filesystem {
                    device: "rootfs".into(),
                    usage: Some(usage),
                    ..Filesystem::default()
                })
                .into_iter()
                .collect(),
            0,
        );
    };
    let base = proc_root.join(pid.to_string()).join("root");
    let mut destinations = BTreeSet::from(["/"]);
    for path in mounts {
        if path.starts_with('/')
            && !Path::new(path)
                .components()
                .any(|c| c == std::path::Component::ParentDir)
        {
            destinations.insert(path);
        }
    }
    let mut errors = 0;
    let values = destinations
        .into_iter()
        .filter_map(|destination| {
            let path = base.join(destination.trim_start_matches('/'));
            let mut value = Filesystem {
                device: if destination == "/" {
                    "rootfs".into()
                } else {
                    destination.into()
                },
                usage: (destination == "/").then_some(writable).flatten(),
                ..Filesystem::default()
            };
            if let Ok(stat) = rustix::fs::statvfs(&path) {
                let size = if stat.f_frsize == 0 {
                    stat.f_bsize
                } else {
                    stat.f_frsize
                };
                value.limit = stat.f_blocks.checked_mul(size);
                value.available = stat.f_bavail.checked_mul(size);
                value.inodes = Some(stat.f_files);
                value.inodes_free = Some(stat.f_ffree);
            } else {
                errors += 1;
            }
            (value.limit.is_some() || value.usage.is_some()).then_some(value)
        })
        .collect();
    (values, errors)
}
