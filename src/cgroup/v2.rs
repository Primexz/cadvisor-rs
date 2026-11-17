//! Unified cgroup v2 resource files.

use std::path::Path;

use crate::model::{Container, Cpu, Memory, PressureSet};

use super::{Reader, cpuset_count, key_values, limit, parse_io_v2, parse_pressure};

pub(super) fn read(path: &Path, reader: &mut Reader) -> Container {
    let cpu_text = reader.text(&path.join("cpu.stat")).unwrap_or_default();
    let cpu_stat = key_values(&cpu_text);
    let cpu_max = reader.text(&path.join("cpu.max")).unwrap_or_default();
    let mut max = cpu_max.split_whitespace();
    let quota = max.next().and_then(|value| {
        if value == "max" {
            Some(-1)
        } else {
            value.parse().ok()
        }
    });
    let period = max.next().and_then(|value| value.parse().ok());
    let weight = reader
        .number(&path.join("cpu.weight"))
        .filter(|v| (1..=10000).contains(v));
    let cpu = Cpu {
        usage: cpu_stat.get("usage_usec").map(|v| *v as f64 / 1e6),
        user: cpu_stat.get("user_usec").map(|v| *v as f64 / 1e6),
        system: cpu_stat.get("system_usec").map(|v| *v as f64 / 1e6),
        periods: cpu_stat.get("nr_periods").copied(),
        throttled_periods: cpu_stat.get("nr_throttled").copied(),
        throttled_seconds: cpu_stat.get("throttled_usec").map(|v| *v as f64 / 1e6),
        quota,
        period,
        weight,
        // cAdvisor's documented linear conversion from v2 weight to v1 shares.
        shares: weight.map(|value| 2 + (value - 1) * 262142 / 9999),
        cpuset_count: reader
            .text(&path.join("cpuset.cpus.effective"))
            .and_then(|v| cpuset_count(&v)),
        burst_periods: cpu_stat.get("nr_bursts").copied(),
        burst_seconds: cpu_stat.get("burst_usec").map(|v| *v as f64 / 1e6),
        ..Cpu::default()
    };
    let usage = reader.number(&path.join("memory.current"));
    let stat_text = reader.text(&path.join("memory.stat")).unwrap_or_default();
    let stat = key_values(&stat_text);
    let events_text = reader.text(&path.join("memory.events")).unwrap_or_default();
    let events = key_values(&events_text);
    let memory_limit = reader
        .text(&path.join("memory.max"))
        .and_then(|v| limit(&v));
    let swap_limit = reader
        .text(&path.join("memory.swap.max"))
        .and_then(|v| limit(&v));
    let mut details = super::memory::details(&stat, false);
    details.peak = reader.number(&path.join("memory.peak"));
    details.reservation = reader
        .text(&path.join("memory.low"))
        .and_then(|v| limit(&v));
    details.high = reader
        .text(&path.join("memory.high"))
        .and_then(|v| limit(&v));
    details.swap_limit = memory_limit
        .zip(swap_limit)
        .and_then(|(a, b)| a.checked_add(b));
    details.events_low = events.get("low").copied();
    details.events_high = events.get("high").copied();
    details.events_max = events.get("max").copied();
    details.events_oom = events.get("oom").copied();
    details.events_oom_group = events.get("oom_group_kill").copied();
    if let Some(local) = reader.text(&path.join("memory.stat.local")) {
        let local = key_values(&local);
        details.faults = [
            local.get("pgfault").copied(),
            local.get("pgmajfault").copied(),
        ];
    }
    let memory = Memory {
        usage,
        working_set: usage
            .zip(stat.get("inactive_file"))
            .map(|(usage, inactive)| usage.saturating_sub(*inactive)),
        rss: stat.get("anon").copied(),
        cache: stat.get("file").copied(),
        swap: reader.number(&path.join("memory.swap.current")),
        limit: memory_limit,
        failures: events.get("max").copied(),
        oom_kills: events.get("oom_kill").copied(),
        details,
    };
    Container {
        cpu,
        memory,
        pids: reader.number(&path.join("pids.current")),
        pids_limit: reader.text(&path.join("pids.max")).and_then(|v| limit(&v)),
        pids_events_max: reader
            .text(&path.join("pids.events"))
            .and_then(|v| key_values(&v).get("max").copied()),
        pressure: PressureSet {
            cpu: reader
                .text(&path.join("cpu.pressure"))
                .map(|v| parse_pressure(&v))
                .unwrap_or_default(),
            memory: reader
                .text(&path.join("memory.pressure"))
                .map(|v| parse_pressure(&v))
                .unwrap_or_default(),
            io: reader
                .text(&path.join("io.pressure"))
                .map(|v| parse_pressure(&v))
                .unwrap_or_default(),
        },
        io: reader
            .text(&path.join("io.stat"))
            .map(|v| parse_io_v2(&v))
            .unwrap_or_default(),
        ..Container::default()
    }
}
