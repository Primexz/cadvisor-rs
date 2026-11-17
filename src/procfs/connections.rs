//! Bounded socket-table snapshots; TCP/UDP state values are gauges despite legacy names.

use crate::model::Connections;
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

/// Parse Linux TCP state codes into eleven cAdvisor-compatible state counts.
/// Returns None for malformed records, without emitting partial totals.
#[must_use]
pub fn parse_tcp(text: &str) -> Option<[u64; 11]> {
    let mut states = [0; 11];
    for line in text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.contains("local_address"))
    {
        let columns: Vec<_> = line.split_whitespace().collect();
        let state = u8::from_str_radix(columns.get(3)?, 16).ok()?;
        if let Some(index) = state.checked_sub(1).filter(|v| *v < 11) {
            states[usize::from(index)] += 1;
        }
    }
    Some(states)
}

/// Parse UDP listen sockets, drops and queue bytes from Linux UDP tables.
#[must_use]
pub fn parse_udp(text: &str) -> Option<[u64; 4]> {
    let mut values = [0u64; 4];
    for line in text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.contains("local_address"))
    {
        let columns: Vec<_> = line.split_whitespace().collect();
        if columns.len() < 13 {
            return None;
        }
        let state = u8::from_str_radix(columns.get(3)?, 16).ok()?;
        let (tx, rx) = columns.get(4)?.split_once(':')?;
        if state == 7 {
            values[0] += 1;
        }
        values[1] = values[1].checked_add(columns.last()?.parse::<u64>().ok()?)?;
        values[2] = values[2].checked_add(u64::from_str_radix(rx, 16).ok()?)?;
        values[3] = values[3].checked_add(u64::from_str_radix(tx, 16).ok()?)?;
    }
    Some(values)
}

fn text(path: &Path) -> io::Result<String> {
    let mut text = String::new();
    File::open(path)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_string(&mut text)?;
    if text.len() > 4 * 1024 * 1024 {
        return Err(io::Error::other("connection table exceeds 4 MiB"));
    }
    Ok(text)
}

/// Read a process's network namespace tables, or root procfs for the host.
#[must_use]
pub fn read_connections(root: &Path, pid: Option<u32>, host: bool) -> Connections {
    let mut result = Connections::default();
    let path = if host {
        root.join("net")
    } else if let Some(pid) = pid {
        root.join(pid.to_string()).join("net")
    } else {
        return result;
    };
    for (name, slot) in [("tcp", &mut result.tcp), ("tcp6", &mut result.tcp6)] {
        match text(&path.join(name)) {
            Ok(text) => {
                *slot = parse_tcp(&text);
                if slot.is_none() {
                    result.errors += 1;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(_) => result.errors += 1,
        }
    }
    for (name, slot) in [("udp", &mut result.udp), ("udp6", &mut result.udp6)] {
        match text(&path.join(name)) {
            Ok(text) => {
                *slot = parse_udp(&text);
                if slot.is_none() {
                    result.errors += 1;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(_) => result.errors += 1,
        }
    }
    result
}
