//! Shared mappings for extended memory counters, with explicit hierarchy scope.

use crate::model::MemoryDetails;
use std::collections::BTreeMap;

pub(super) fn details(stat: &BTreeMap<&str, u64>, v1: bool) -> MemoryDetails {
    let value = |name: &str| {
        if v1 {
            stat.get(format!("total_{name}").as_str())
                .or_else(|| stat.get(name))
                .copied()
        } else {
            stat.get(name).copied()
        }
    };
    MemoryDetails {
        mapped_file: value(if v1 { "mapped_file" } else { "file_mapped" }),
        kernel: value("kernel"),
        active_file: value("active_file"),
        inactive_file: value("inactive_file"),
        dirty: value(if v1 { "dirty" } else { "file_dirty" }),
        writeback: value(if v1 { "writeback" } else { "file_writeback" }),
        socket: value("sock"),
        slab: value("slab"),
        anon_thp: value(if v1 { "rss_huge" } else { "anon_thp" }),
        pgscan: value("pgscan"),
        pgsteal: value("pgsteal"),
        refault_file: value("workingset_refault_file"),
        refault_anon: value("workingset_refault_anon"),
        faults: if v1 {
            [
                stat.get("pgfault").copied(),
                stat.get("pgmajfault").copied(),
            ]
        } else {
            [None; 2]
        },
        hierarchical_faults: if v1 {
            [
                stat.get("total_pgfault").copied(),
                stat.get("total_pgmajfault").copied(),
            ]
        } else {
            [
                stat.get("pgfault").copied(),
                stat.get("pgmajfault").copied(),
            ]
        },
        ..MemoryDetails::default()
    }
}
