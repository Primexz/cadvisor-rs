//! Per-cgroup v1 block scheduler statistics with CFQ/BFQ fallbacks.

use std::{collections::BTreeMap, path::Path};

use crate::model::Io;

use super::{Reader, parse::valid_device};

#[derive(Clone, Copy)]
enum Stat {
    Bytes,
    Operations,
    ServiceTime,
    Merged,
    Queued,
}

pub(super) fn read(path: &Path, reader: &mut Reader) -> BTreeMap<String, Io> {
    let mut io = BTreeMap::new();
    for (name, kind) in [
        ("io_service_bytes", Stat::Bytes),
        ("io_serviced", Stat::Operations),
        ("io_service_time", Stat::ServiceTime),
        ("io_merged", Stat::Merged),
        ("io_queued", Stat::Queued),
    ] {
        let text = if matches!(kind, Stat::Bytes | Stat::Operations) {
            reader.text(&path.join(format!("blkio.throttle.{name}")))
        } else {
            None
        };
        let text = text.or_else(|| {
            [
                format!("blkio.{name}_recursive"),
                format!("blkio.bfq.{name}_recursive"),
                format!("blkio.{name}"),
                format!("blkio.bfq.{name}"),
            ]
            .iter()
            .find_map(|name| reader.text(&path.join(name)))
        });
        if let Some(text) = text {
            parse(&text, kind, &mut io);
        }
    }
    io
}

fn parse(text: &str, kind: Stat, io: &mut BTreeMap<String, Io>) {
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(device), Some(operation), Some(value)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if fields.next().is_some() || !valid_device(device) {
            continue;
        }
        let Ok(value) = value.parse::<u64>() else {
            continue;
        };
        let supported = match kind {
            Stat::Bytes | Stat::Operations => matches!(operation, "Read" | "Write" | "Discard"),
            Stat::ServiceTime => matches!(operation, "Read" | "Write" | "Total"),
            Stat::Merged => matches!(operation, "Read" | "Write"),
            Stat::Queued => operation == "Total",
        };
        if !supported {
            continue;
        }
        let entry = io.entry(device.to_owned()).or_default();
        match (kind, operation) {
            (Stat::Bytes, "Read") => entry.read_bytes = Some(value),
            (Stat::Bytes, "Write") => entry.write_bytes = Some(value),
            (Stat::Bytes, "Discard") => entry.discard_bytes = Some(value),
            (Stat::Operations, "Read") => entry.reads = Some(value),
            (Stat::Operations, "Write") => entry.writes = Some(value),
            (Stat::Operations, "Discard") => entry.discards = Some(value),
            (Stat::ServiceTime, "Read") => entry.read_seconds = Some(value as f64 / 1e9),
            (Stat::ServiceTime, "Write") => entry.write_seconds = Some(value as f64 / 1e9),
            (Stat::ServiceTime, "Total") => entry.service_seconds = Some(value as f64 / 1e9),
            (Stat::Merged, "Read") => entry.reads_merged = Some(value),
            (Stat::Merged, "Write") => entry.writes_merged = Some(value),
            (Stat::Queued, "Total") => entry.io_current = Some(value),
            _ => unreachable!("supported operation was checked"),
        }
    }
}
