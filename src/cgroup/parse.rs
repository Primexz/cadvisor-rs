//! Pure parsers used by both cgroup layouts and fixture tests.

use std::collections::BTreeMap;

use crate::model::{Io, Pressure, PressureLine};

/// Parse kernel `key value` records, ignoring unknown or malformed lines.
///
/// ```
/// use cadvisor_rs::cgroup::key_values;
/// assert_eq!(key_values("usage_usec 42\n")["usage_usec"], 42);
/// ```
#[must_use]
pub fn key_values(text: &str) -> BTreeMap<&str, u64> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?, fields.next()?.parse().ok()?))
        })
        .collect()
}

/// Parse a finite kernel limit; `max`, negative and v1 sentinel values are absent.
///
/// ```
/// use cadvisor_rs::cgroup::limit;
/// assert_eq!(limit("max"), None);
/// assert_eq!(limit("4096\n"), Some(4096));
/// ```
#[must_use]
pub fn limit(text: &str) -> Option<u64> {
    text.trim()
        .parse::<u64>()
        .ok()
        .filter(|value| *value < (1u64 << 60))
}

/// Extract a full runtime container ID from Docker, containerd or CRI-O paths.
///
/// Only complete path components are considered, so service names containing a
/// hex substring cannot accidentally inherit another container's metadata.
#[must_use]
pub fn container_id(path: &str) -> Option<&str> {
    path.split('/').rev().find_map(|part| {
        let part = part.strip_suffix(".scope").unwrap_or(part);
        let part = ["docker-", "cri-containerd-", "crio-"]
            .iter()
            .find_map(|prefix| part.strip_prefix(prefix))
            .unwrap_or(part);
        (part.len() == 64 && part.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(part)
    })
}

/// Parse unified cgroup block I/O counters, preserving absent per-device fields.
#[must_use]
pub fn parse_io_v2(text: &str) -> BTreeMap<String, Io> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let device = fields.next()?;
            if !valid_device(device) {
                return None;
            }
            let mut io = Io::default();
            let mut available = false;
            for field in fields {
                let Some((key, value)) = field.split_once('=') else {
                    continue;
                };
                let Ok(value) = value.parse::<u64>() else {
                    continue;
                };
                match key {
                    "rbytes" => io.read_bytes = Some(value),
                    "wbytes" => io.write_bytes = Some(value),
                    "rios" => io.reads = Some(value),
                    "wios" => io.writes = Some(value),
                    "dbytes" => io.discard_bytes = Some(value),
                    "dios" => io.discards = Some(value),
                    "cost.usage" => io.cost_usage_seconds = Some(value as f64 / 1e6),
                    "cost.wait" => io.cost_wait_seconds = Some(value as f64 / 1e6),
                    "cost.indebt" => io.cost_indebt_seconds = Some(value as f64 / 1e6),
                    "cost.indelay" => io.cost_indelay_seconds = Some(value as f64 / 1e6),
                    _ => continue,
                }
                available = true;
            }
            available.then(|| (device.to_owned(), io))
        })
        .collect()
}

/// Count CPUs in a kernel CPU list, merging overlapping ranges without expanding them.
///
/// ```
/// assert_eq!(cadvisor_rs::cgroup::cpuset_count("0-3,2-5,8"), Some(7));
/// ```
#[must_use]
pub fn cpuset_count(text: &str) -> Option<u64> {
    let mut ranges = Vec::new();
    for part in text.trim().split(',') {
        let (a, b) = part.split_once('-').unwrap_or((part, part));
        let a = a.parse::<u32>().ok()?;
        let b = b.parse::<u32>().ok()?;
        if a > b || b > 1_048_575 {
            return None;
        }
        ranges.push((a, b));
    }
    ranges.sort_unstable();
    let mut merged: Option<(u32, u32)> = None;
    let mut count = 0;
    for (a, b) in ranges {
        match merged {
            Some((start, end)) if a <= end + 1 => merged = Some((start, end.max(b))),
            Some((start, end)) => {
                count += u64::from(end - start + 1);
                merged = Some((a, b));
            }
            None => merged = Some((a, b)),
        }
    }
    merged.map(|(a, b)| count + u64::from(b - a + 1))
}

/// Parse PSI percentages and convert cumulative microseconds into seconds.
/// Unknown rows/fields and invalid or non-finite averages are ignored.
#[must_use]
pub fn parse_pressure(text: &str) -> Pressure {
    let mut result = Pressure::default();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let row = match fields.next() {
            Some("some") => &mut result.some,
            Some("full") => &mut result.full,
            _ => continue,
        };
        let mut parsed = PressureLine::default();
        for field in fields {
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            if key == "total" {
                parsed.total = value.parse::<u64>().ok().map(|v| v as f64 / 1e6);
            } else if let Some(i) = ["avg10", "avg60", "avg300"].iter().position(|k| *k == key) {
                parsed.averages[i] = value
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite() && (0.0..=100.0).contains(v));
            }
        }
        if parsed.total.is_some() || parsed.averages.iter().any(Option::is_some) {
            *row = Some(parsed);
        }
    }
    result
}

pub(super) fn valid_device(device: &str) -> bool {
    device
        .split_once(':')
        .is_some_and(|(major, minor)| major.parse::<u32>().is_ok() && minor.parse::<u32>().is_ok())
}
