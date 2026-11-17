//! Split cgroup v1 controller files, including co-mounted CPU controllers.

use std::path::Path;

use crate::model::{Container, Cpu, Memory};

use super::{Group, Reader, Version, controller, cpuset_count, key_values, limit};

pub(super) fn read(root: &Path, group: &Group, reader: &mut Reader) -> Container {
    let relative = group.id.trim_start_matches('/');
    let cpu_path = controller(root, &["cpu", "cpu,cpuacct", "cpuacct,cpu"]).join(relative);
    let acct = controller(root, &["cpuacct", "cpu,cpuacct", "cpuacct,cpu"]).join(relative);
    let memory_path = root.join("memory").join(relative);
    let pids = root.join("pids").join(relative);
    let blkio = root.join("blkio").join(relative);
    let stat_text = reader.text(&cpu_path.join("cpu.stat")).unwrap_or_default();
    let stat = key_values(&stat_text);
    let cpu = Cpu {
        usage: reader
            .number(&acct.join("cpuacct.usage"))
            .map(|v| v as f64 / 1e9),
        user: reader
            .number(&acct.join("cpuacct.usage_user"))
            .map(|v| v as f64 / 1e9),
        system: reader
            .number(&acct.join("cpuacct.usage_sys"))
            .map(|v| v as f64 / 1e9),
        periods: stat.get("nr_periods").copied(),
        throttled_periods: stat.get("nr_throttled").copied(),
        throttled_seconds: stat.get("throttled_time").map(|v| *v as f64 / 1e9),
        quota: reader.signed(&cpu_path.join("cpu.cfs_quota_us")),
        period: reader.number(&cpu_path.join("cpu.cfs_period_us")),
        shares: reader.number(&cpu_path.join("cpu.shares")),
        burst_periods: stat.get("nr_bursts").copied(),
        burst_seconds: stat.get("burst_time").map(|v| *v as f64 / 1e9),
        cpuset_count: reader
            .text(
                &root
                    .join("cpuset")
                    .join(relative)
                    .join("cpuset.effective_cpus"),
            )
            .or_else(|| reader.text(&root.join("cpuset").join(relative).join("cpuset.cpus")))
            .and_then(|v| cpuset_count(&v)),
        per_cpu: reader
            .text(&acct.join("cpuacct.usage_percpu"))
            .and_then(|v| {
                v.split_whitespace()
                    .map(str::parse::<u64>)
                    .collect::<Result<Vec<_>, _>>()
                    .ok()
            })
            .unwrap_or_default()
            .into_iter()
            .map(|v| v as f64 / 1e9)
            .collect(),
        ..Cpu::default()
    };
    let usage = reader.number(&memory_path.join("memory.usage_in_bytes"));
    let text = reader
        .text(&memory_path.join("memory.stat"))
        .unwrap_or_default();
    let stat = key_values(&text);
    let get = |total, local| stat.get(total).or_else(|| stat.get(local)).copied();
    let mut details = super::memory::details(&stat, true);
    details.peak = reader.number(&memory_path.join("memory.max_usage_in_bytes"));
    details.kernel = reader
        .number(&memory_path.join("memory.kmem.usage_in_bytes"))
        .or(details.kernel);
    details.reservation = reader
        .text(&memory_path.join("memory.soft_limit_in_bytes"))
        .and_then(|v| limit(&v));
    details.swap_limit = reader
        .text(&memory_path.join("memory.memsw.limit_in_bytes"))
        .and_then(|v| limit(&v));
    details.numa = super::numa::read(&memory_path, Version::V1, reader);
    details.migrate = reader
        .number(
            &root
                .join("cpuset")
                .join(relative)
                .join("cpuset.memory_migrate"),
        )
        .filter(|value| *value <= 1)
        .map(|value| value == 1);
    let swap = get("total_swap", "swap").or_else(|| {
        reader
            .number(&memory_path.join("memory.memsw.usage_in_bytes"))
            .zip(usage)
            .map(|(memsw, usage)| memsw.saturating_sub(usage))
    });
    let memory = Memory {
        usage,
        working_set: usage
            .zip(get("total_inactive_file", "inactive_file"))
            .map(|(usage, inactive)| usage.saturating_sub(inactive)),
        rss: get("total_rss", "rss"),
        cache: get("total_cache", "cache"),
        swap,
        limit: reader
            .text(&memory_path.join("memory.limit_in_bytes"))
            .and_then(|v| limit(&v)),
        failures: reader.number(&memory_path.join("memory.failcnt")),
        oom_kills: None,
        details,
    };
    Container {
        cpu,
        memory,
        hugetlb: super::hugetlb::read(&root.join("hugetlb").join(relative), Version::V1, reader),
        pids: reader.number(&pids.join("pids.current")),
        pids_limit: reader.text(&pids.join("pids.max")).and_then(|v| limit(&v)),
        pids_events_max: reader
            .text(&pids.join("pids.events"))
            .and_then(|v| key_values(&v).get("max").copied()),
        io: super::blkio::read(&blkio, reader),
        ..Container::default()
    }
}
