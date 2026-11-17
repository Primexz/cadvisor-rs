//! End-to-end fixture coverage of discovery, units, optional fields and churn.

mod common;

use std::{collections::BTreeMap, fs};

use cadvisor_rs::{
    cgroup::{self, Version},
    collector::Collector,
    metrics::{self, Labels},
    model::Metadata,
};
use common::{ID, config, fixture, put, v1, v2};

#[test]
fn unified_collection_and_metadata_have_correct_units() {
    let temp = fixture();
    let path = format!("system.slice/docker-{ID}.scope");
    v2(temp.path(), &path, 3_000_000);
    let metadata = BTreeMap::from([(
        ID.to_owned(),
        Metadata {
            name: "web\"\\\nservice".to_owned(),
            image: "demo:latest".to_owned(),
            created: Some(123),
            labels: BTreeMap::from([("app.role".into(), "test\nrole".into())]),
            ..Metadata::default()
        },
    )]);
    let sample = Collector::new(config(temp.path()))
        .unwrap()
        .collect(&metadata, true)
        .unwrap();
    assert_eq!(sample.cgroup_version, "2");
    assert_eq!(sample.errors, 0);
    assert_eq!(sample.containers.len(), 1);
    let c = &sample.containers[0];
    assert_eq!(c.cpu.usage, Some(3.0));
    assert_eq!(c.cpu.user, Some(2.0));
    assert_eq!(c.cpu.system, Some(1.0));
    assert_eq!(c.cpu.quota, Some(200000));
    assert_eq!(c.cpu.period, Some(100000));
    assert_eq!(c.cpu.throttled_seconds, Some(0.125));
    assert_eq!(c.memory.usage, Some(4096));
    assert_eq!(c.memory.working_set, Some(2048));
    assert_eq!(c.memory.rss, Some(1024));
    assert_eq!(c.memory.cache, Some(3072));
    assert_eq!(c.memory.swap, Some(128));
    assert_eq!(c.memory.limit, Some(8192 * 1024));
    assert_eq!(c.memory.failures, Some(3));
    assert_eq!(c.memory.oom_kills, Some(1));
    assert_eq!(c.pids, Some(7));
    assert_eq!(c.io["8:0"].write_bytes, Some(8192));
    assert_eq!(c.io["8:0"].reads, Some(2));
    assert_eq!(c.network["eth0"].rx_bytes, 100);
    assert_eq!(c.network["eth0"].tx_bytes, 200);
    assert!(!c.network.contains_key("lo"));
    let text = String::from_utf8(
        metrics::encode(&sample, &Labels::new(&["app.role".into()]).unwrap()).to_vec(),
    )
    .unwrap();
    assert!(text.contains("name=\"web\\\"\\\\\\nservice\""));
    assert!(text.contains("container_label_app_role=\"test\\nrole\""));
    assert!(text.contains("cpu=\"total\"} 3\n"));
    assert_eq!(sample.machine.cpu_cores, Some(2));
}

#[test]
fn split_controllers_match_unified_resource_values() {
    let a = fixture();
    let b = fixture();
    v1(a.path(), &format!("docker/{ID}"), 3_000_000_000);
    v2(b.path(), &format!("docker/{ID}"), 3_000_000);
    let legacy = Collector::new(config(a.path()))
        .unwrap()
        .collect(&BTreeMap::new(), false)
        .unwrap();
    let unified = Collector::new(config(b.path()))
        .unwrap()
        .collect(&BTreeMap::new(), false)
        .unwrap();
    let legacy = &legacy.containers[0];
    let unified = &unified.containers[0];
    assert_eq!(legacy.cpu.usage, unified.cpu.usage);
    assert_eq!(legacy.cpu.user, unified.cpu.user);
    assert_eq!(legacy.cpu.throttled_seconds, unified.cpu.throttled_seconds);
    assert_eq!(legacy.memory.working_set, unified.memory.working_set);
    assert_eq!(legacy.memory.swap, unified.memory.swap);
    assert_eq!(legacy.memory.rss, unified.memory.rss);
    assert_eq!(legacy.io["8:0"].read_bytes, unified.io["8:0"].read_bytes);
    assert_eq!(legacy.io["8:0"].writes, unified.io["8:0"].writes);
}

#[test]
fn missing_controllers_and_unlimited_values_stay_absent() {
    let temp = fixture();
    put(temp.path(), "cgroup/cgroup.controllers", "cpu\n");
    put(temp.path(), "cgroup/cpu.stat", "usage_usec 0\n");
    put(temp.path(), "cgroup/cpu.max", "max 100000\n");
    put(temp.path(), "cgroup/pids.max", "max\n");
    let sample = Collector::new(config(temp.path()))
        .unwrap()
        .collect(&BTreeMap::new(), false)
        .unwrap();
    let c = &sample.containers[0];
    assert_eq!(c.cpu.usage, Some(0.0));
    assert_eq!(c.cpu.quota, Some(-1));
    assert_eq!(c.pids_limit, None);
    assert_eq!(c.memory.limit, None);
    assert_eq!(c.memory.usage, None);
}

#[test]
fn malformed_numbers_are_counted_and_working_set_never_underflows() {
    let temp = fixture();
    v2(temp.path(), "", 1);
    put(temp.path(), "cgroup/memory.current", "512\n");
    put(temp.path(), "cgroup/pids.current", "invalid\n");
    let sample = Collector::new(config(temp.path()))
        .unwrap()
        .collect(&BTreeMap::new(), false)
        .unwrap();
    assert_eq!(sample.errors, 1);
    assert_eq!(sample.containers[0].memory.working_set, Some(0));
    assert_eq!(sample.containers[0].pids, None);
}

#[test]
fn discovery_is_bounded_and_containers_filter_is_precise() {
    let temp = fixture();
    v2(temp.path(), &format!("docker/{ID}"), 1);
    assert!(cgroup::discover(&temp.path().join("cgroup"), Version::V2, false, 2, 64).is_err());
    assert!(cgroup::discover(&temp.path().join("cgroup"), Version::V2, false, 10, 1).is_err());
    let groups = cgroup::discover(&temp.path().join("cgroup"), Version::V2, true, 10, 64).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].id, format!("/docker/{ID}"));
}

#[test]
fn cached_discovery_handles_removal_and_finds_new_groups_on_refresh() {
    let temp = fixture();
    v2(temp.path(), "", 1);
    let mut collector = Collector::new(config(temp.path())).unwrap();
    assert_eq!(
        collector
            .collect(&BTreeMap::new(), false)
            .unwrap()
            .containers
            .len(),
        1
    );
    v2(temp.path(), "new", 2);
    assert_eq!(
        collector
            .collect(&BTreeMap::new(), false)
            .unwrap()
            .containers
            .len(),
        1
    );
    let mut collector = Collector::new(config(temp.path())).unwrap();
    assert_eq!(
        collector
            .collect(&BTreeMap::new(), false)
            .unwrap()
            .containers
            .len(),
        2
    );
    fs::remove_dir_all(temp.path().join("cgroup/new")).unwrap();
    assert_eq!(
        collector
            .collect(&BTreeMap::new(), false)
            .unwrap()
            .containers
            .len(),
        1
    );
}

#[test]
fn empty_selection_succeeds_and_missing_mount_fails() {
    let temp = fixture();
    assert!(Collector::new(config(temp.path())).is_err());
    v2(temp.path(), "", 1);
    let mut cfg = config(temp.path());
    cfg.containers_only = true;
    let mut collector = Collector::new(cfg).unwrap();
    assert!(
        collector
            .collect(&BTreeMap::new(), false)
            .unwrap()
            .containers
            .is_empty()
    );
    fs::remove_dir_all(temp.path().join("cgroup")).unwrap();
    assert!(collector.collect(&BTreeMap::new(), false).is_err());
}

#[cfg(unix)]
#[test]
fn discovery_does_not_follow_child_symlinks() {
    let temp = fixture();
    v2(temp.path(), "", 1);
    std::os::unix::fs::symlink(temp.path().join("cgroup"), temp.path().join("cgroup/cycle"))
        .unwrap();
    let groups = cgroup::discover(&temp.path().join("cgroup"), Version::V2, false, 10, 10).unwrap();
    assert_eq!(groups.len(), 1);
}
