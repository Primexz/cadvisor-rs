//! NUMA residency with explicit page units and hierarchy scope.

use std::{collections::BTreeMap, path::Path};

use crate::model::{NumaNode, NumaStats};

use super::{Reader, Version};

/// Parse v1 NUMA page counts, retaining local and hierarchical rows separately.
#[must_use]
pub fn parse_numa_v1(text: &str) -> NumaStats {
    let mut stats = NumaStats::default();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some((kind, total)) = fields.next().and_then(|value| value.split_once('=')) else {
            continue;
        };
        if total.parse::<u64>().is_err() {
            continue;
        }
        let (nodes, kind) = if let Some(kind) = kind.strip_prefix("hierarchical_") {
            (&mut stats.hierarchy, kind)
        } else {
            (&mut stats.container, kind)
        };
        parse_nodes(nodes, kind, fields, 1);
    }
    stats
}

/// Convert v2 NUMA bytes to base pages using the host's page size.
/// Unified memory statistics include descendants; a zero page size omits all data.
#[must_use]
pub fn parse_numa_v2(text: &str, page_size: u64) -> NumaStats {
    let mut stats = NumaStats::default();
    if page_size == 0 {
        return stats;
    }
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        if let Some(kind) = fields.next() {
            parse_nodes(&mut stats.hierarchy, kind, fields, page_size);
        }
    }
    stats
}

fn parse_nodes<'a>(
    nodes: &mut BTreeMap<u32, NumaNode>,
    kind: &str,
    fields: impl Iterator<Item = &'a str>,
    divisor: u64,
) {
    if !matches!(kind, "anon" | "file" | "unevictable") {
        return;
    }
    for field in fields {
        let Some((node, value)) = field.split_once('=') else {
            continue;
        };
        let Some(node) = node.strip_prefix('N').and_then(|node| node.parse().ok()) else {
            continue;
        };
        let Ok(value) = value.parse::<u64>() else {
            continue;
        };
        let node = nodes.entry(node).or_default();
        let slot = match kind {
            "anon" => &mut node.anon,
            "file" => &mut node.file,
            _ => &mut node.unevictable,
        };
        *slot = Some(value / divisor);
    }
}

pub(super) fn read(path: &Path, version: Version, reader: &mut Reader) -> NumaStats {
    let Some(text) = reader.text(&path.join("memory.numa_stat")) else {
        return NumaStats::default();
    };
    match version {
        Version::V1 => parse_numa_v1(&text),
        Version::V2 => parse_numa_v2(&text, rustix::param::page_size() as u64),
    }
}
