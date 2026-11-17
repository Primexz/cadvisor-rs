//! Extended controller fields and inventory/resource separation across state changes.
mod common;

use cadvisor_rs::{
    cgroup,
    collector::Collector,
    metrics::{self, Labels},
    model::{ContainerState, DockerDetails, Metadata},
};
use common::{ID, config, fixture, put, v1, v2};
use std::{collections::BTreeMap, fs};

#[test]
fn unified_memory_events_limits_pressure_and_burst_counters_are_exposed() {
    let temp = fixture();
    v2(temp.path(), "work", 3_000_000);
    let path = temp.path().join("cgroup/work");
    put(
        &path,
        "cpu.stat",
        "usage_usec 3000000\nuser_usec 2000000\nsystem_usec 1000000\nnr_bursts 3\nburst_usec 250000\n",
    );
    put(&path, "cpu.weight", "100\n");
    put(&path, "cpuset.cpus.effective", "0-3,2-5,8\n");
    put(
        &path,
        "memory.stat",
        "anon 1024\nfile 3072\ninactive_file 2048\nactive_file 512\nfile_mapped 256\nkernel 128\nfile_dirty 64\nfile_writeback 32\nsock 16\nslab 8\nanon_thp 4096\npgfault 42\npgmajfault 3\npgscan 11\npgsteal 7\nworkingset_refault_file 5\nworkingset_refault_anon 4\n",
    );
    put(&path, "memory.stat.local", "pgfault 9\npgmajfault 2\n");
    put(
        &path,
        "memory.events",
        "low 1\nhigh 2\nmax 3\noom 4\noom_kill 5\noom_group_kill 6\n",
    );
    for (name, value) in [
        ("memory.peak", "8192"),
        ("memory.max", "8192"),
        ("memory.low", "1024"),
        ("memory.high", "6144"),
        ("memory.swap.max", "4096"),
    ] {
        put(&path, name, value);
    }
    put(&path, "pids.events", "max 4\n");
    let pressure = "some avg10=12.5 avg60=6.0 avg300=2.5 total=2500000\nfull avg10=1.0 avg60=0.5 avg300=0.25 total=500000\n";
    for name in ["cpu.pressure", "memory.pressure", "io.pressure"] {
        put(&path, name, pressure);
    }
    let sample = Collector::new(config(temp.path()))
        .unwrap()
        .collect(&BTreeMap::new(), false)
        .unwrap();
    let c = &sample.containers[0];
    assert_eq!(c.cpu.shares, Some(2597));
    assert_eq!(c.cpu.weight, Some(100));
    assert_eq!(c.cpu.cpuset_count, Some(7));
    assert_eq!(c.cpu.burst_seconds, Some(0.25));
    assert_eq!(c.cpu.burst_periods, Some(3));
    let d = &c.memory.details;
    assert_eq!(d.peak, Some(8192));
    assert_eq!(d.swap_limit, Some(12288));
    assert_eq!(d.reservation, Some(1024));
    assert_eq!(d.mapped_file, Some(256));
    assert_eq!(d.faults, [Some(9), Some(2)]);
    assert_eq!(d.hierarchical_faults, [Some(42), Some(3)]);
    assert_eq!(d.events_oom, Some(4));
    assert_eq!(c.pressure.cpu.some.as_ref().unwrap().total, Some(2.5));
    let text = String::from_utf8(metrics::encode(&sample, &Labels::default()).to_vec()).unwrap();
    for (name, value) in [
        ("container_spec_cpu_shares", "2597"),
        ("container_memory_file_dirty_bytes", "64"),
        ("container_memory_events_high_total", "2"),
        ("container_pressure_io_stalled_seconds_total", "0.5"),
        ("container_pressure_cpu_waiting_avg10", "12.5"),
        ("container_pids_events_max_total", "4"),
    ] {
        assert!(
            text.lines()
                .any(|line| line.starts_with(&format!("{name}{{"))
                    && line.ends_with(&format!("}} {value}"))),
            "missing {name}"
        );
    }
    assert!(text.contains("failure_type=\"pgfault\",scope=\"container\"} 9"));
    assert!(text.contains("failure_type=\"pgfault\",scope=\"hierarchy\"} 42"));
}

#[test]
fn legacy_memory_hierarchy_and_per_cpu_usage_retain_units_and_scope() {
    let temp = fixture();
    v1(temp.path(), "work", 3_000_000_000);
    let memory = temp.path().join("cgroup/memory/work");
    put(
        &memory,
        "memory.stat",
        "mapped_file 1\ntotal_mapped_file 256\ndirty 1\ntotal_dirty 64\npgfault 9\npgmajfault 2\ntotal_pgfault 42\ntotal_pgmajfault 3\n",
    );
    put(&memory, "memory.max_usage_in_bytes", "8192");
    put(&memory, "memory.kmem.usage_in_bytes", "128");
    put(&memory, "memory.soft_limit_in_bytes", "1024");
    put(&memory, "memory.memsw.limit_in_bytes", "12288");
    put(
        &temp.path().join("cgroup/cpu,cpuacct/work"),
        "cpuacct.usage_percpu",
        "1000000000 2000000000\n",
    );
    put(
        &temp.path().join("cgroup/cpu,cpuacct/work"),
        "cpu.shares",
        "1024",
    );
    let sample = Collector::new(config(temp.path()))
        .unwrap()
        .collect(&BTreeMap::new(), false)
        .unwrap();
    let c = &sample.containers[0];
    assert_eq!(c.cpu.per_cpu, [1.0, 2.0]);
    assert_eq!(c.cpu.shares, Some(1024));
    assert_eq!(c.memory.details.mapped_file, Some(256));
    assert_eq!(c.memory.details.kernel, Some(128));
    assert_eq!(c.memory.details.faults, [Some(9), Some(2)]);
    assert_eq!(c.memory.details.hierarchical_faults, [Some(42), Some(3)]);
    assert_eq!(c.memory.details.swap_limit, Some(12288));
}

#[test]
fn stopped_and_created_inventory_remain_without_fake_resource_counters() {
    let temp = fixture();
    let relative = format!("system.slice/docker-{ID}.scope");
    v2(temp.path(), &relative, 1);
    let second = format!("{:064x}", 2);
    let mut metadata = BTreeMap::from([
        (
            ID.into(),
            Metadata {
                name: "web".into(),
                state: Some(ContainerState::Running),
                details: DockerDetails {
                    running: Some(true),
                    pid: Some(123),
                    ..DockerDetails::default()
                },
                ..Metadata::default()
            },
        ),
        (
            second.clone(),
            Metadata {
                name: "created".into(),
                state: Some(ContainerState::Created),
                details: DockerDetails {
                    running: Some(false),
                    memory: Some(1048576),
                    ..DockerDetails::default()
                },
                ..Metadata::default()
            },
        ),
    ]);
    let mut cfg = config(temp.path());
    cfg.containers_only = true;
    let mut collector = Collector::new(cfg).unwrap();
    let sample = collector.collect(&metadata, true).unwrap();
    assert_eq!(sample.containers.len(), 2);
    let created = sample
        .containers
        .iter()
        .find(|c| c.metadata.name == "created")
        .unwrap();
    assert!(!created.has_resources());
    assert_eq!(created.memory.limit, Some(1048576));
    assert_eq!(created.cpu.usage, None);
    fs::remove_dir_all(temp.path().join("cgroup").join(&relative)).unwrap();
    let item = metadata.get_mut(ID).unwrap();
    item.state = Some(ContainerState::Exited);
    item.details.running = Some(false);
    item.details.exit_code = Some(7);
    let sample = collector.collect(&metadata, true).unwrap();
    let stopped = sample
        .containers
        .iter()
        .find(|c| c.metadata.name == "web")
        .unwrap();
    assert_eq!(stopped.id, format!("/{relative}"));
    assert!(!stopped.has_resources());
    assert_eq!(stopped.metadata.details.exit_code, Some(7));
    let text = String::from_utf8(metrics::encode(&sample, &Labels::default()).to_vec()).unwrap();
    assert!(text.contains("state=\"exited\"} 1"));
    assert!(
        !text
            .lines()
            .any(|line| line.starts_with("container_cpu_usage_seconds_total{"))
    );
    assert!(
        text.lines()
            .any(|line| line.starts_with("container_running{")
                && line.contains("name=\"web\"")
                && line.ends_with("} 0"))
    );
    metadata.remove(ID);
    assert_eq!(
        collector.collect(&metadata, true).unwrap().containers.len(),
        1
    );
}

#[test]
fn pressure_and_cpuset_parsers_reject_invalid_nonfinite_and_unbounded_values() {
    assert_eq!(cgroup::cpuset_count("0-1048576"), None);
    assert_eq!(cgroup::cpuset_count("5-2"), None);
    assert_eq!(cgroup::cpuset_count("0,2-4,3-5"), Some(5));
    let value = cgroup::parse_pressure(
        "some avg10=NaN avg60=101 avg300=-1 total=20\nunknown avg10=1 total=9\n",
    );
    assert_eq!(value.some.unwrap().averages, [None; 3]);
    assert!(value.full.is_none());
}
