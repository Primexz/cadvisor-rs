//! Host capacity and network namespace counters from procfs.

use std::{collections::BTreeMap, fs, path::Path};

use crate::model::{Machine, Network};

mod connections;
mod filesystem;
mod process;
mod tcp_stats;
pub use connections::{parse_tcp, parse_udp, read_connections};
pub use filesystem::read_filesystems;
pub use process::{parse_ulimits, read_processes};
pub use tcp_stats::parse_advanced_tcp;

/// Read distinct PIDs across a cgroup subtree without unbounded process walks.
///
/// # Errors
/// Returns an error for unreadable/malformed membership or exceeding the bound.
pub fn pids(paths: &[std::path::PathBuf], maximum: u32) -> std::io::Result<Vec<u32>> {
    let mut ids = std::collections::BTreeSet::new();
    let mut read_any = false;
    for path in paths {
        let text = match fs::read_to_string(path.join("cgroup.procs")) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        read_any = true;
        for line in text.lines() {
            let pid = line.trim().parse::<u32>().map_err(std::io::Error::other)?;
            if pid != 0 {
                ids.insert(pid);
            }
            if ids.len() > maximum as usize {
                return Err(std::io::Error::other(
                    "process count exceeds --max-processes",
                ));
            }
        }
    }
    if !read_any {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cgroup process membership unavailable",
        ));
    }
    Ok(ids.into_iter().collect())
}

/// Parse /proc/net/dev and omit loopback, truncated or malformed records.
#[must_use]
pub fn parse_network(text: &str) -> BTreeMap<String, Network> {
    text.lines()
        .filter_map(|line| {
            let (interface, counters) = line.split_once(':')?;
            let interface = interface.trim();
            if interface == "lo" || interface.is_empty() {
                return None;
            }
            let fields: Vec<u64> = counters
                .split_whitespace()
                .map(str::parse)
                .collect::<Result<_, _>>()
                .ok()?;
            if fields.len() != 16 {
                return None;
            }
            Some((
                interface.to_owned(),
                Network {
                    rx_bytes: fields[0],
                    rx_packets: fields[1],
                    rx_errors: fields[2],
                    rx_dropped: fields[3],
                    tx_bytes: fields[8],
                    tx_packets: fields[9],
                    tx_errors: fields[10],
                    tx_dropped: fields[11],
                },
            ))
        })
        .collect()
}

/// Read machine capacity. Unsupported or unreadable values remain absent.
#[must_use]
pub fn machine(root: &Path) -> Machine {
    let memory = fs::read_to_string(root.join("meminfo")).unwrap_or_default();
    let kib = |name| {
        memory.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            if key != name {
                return None;
            }
            let mut fields = value.split_whitespace();
            let number = fields.next()?.parse::<u64>().ok()?;
            (fields.next()? == "kB")
                .then(|| number.checked_mul(1024))
                .flatten()
        })
    };
    let stat = fs::read_to_string(root.join("stat")).ok();
    Machine {
        cpu_cores: stat
            .map(|text| {
                text.lines()
                    .filter(|line| {
                        line.strip_prefix("cpu")
                            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
                    })
                    .count() as u64
            })
            .filter(|count| *count > 0),
        memory_bytes: kib("MemTotal"),
        swap_bytes: kib("SwapTotal"),
    }
}

/// Read the first live process network namespace in a cgroup, or host networking
/// for the root cgroup. Processes that exit during sampling are skipped.
#[must_use]
pub fn network(proc_root: &Path, cgroup: &Path, is_root: bool) -> BTreeMap<String, Network> {
    if is_root {
        return fs::read_to_string(proc_root.join("net/dev"))
            .map(|v| parse_network(&v))
            .unwrap_or_default();
    }
    let processes = fs::read_to_string(cgroup.join("cgroup.procs")).unwrap_or_default();
    for pid in processes
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .filter(|pid| *pid > 0)
        .take(32)
    {
        if let Ok(text) = fs::read_to_string(proc_root.join(pid.to_string()).join("net/dev")) {
            return parse_network(&text);
        }
    }
    BTreeMap::new()
}
