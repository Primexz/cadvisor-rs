//! Exposition contract for the additional process, network, filesystem and daemon data.
#![cfg(unix)]
mod common;

use cadvisor_rs::{
    collector::Collector,
    metrics::{self, Labels},
    model::{ContainerState, DockerDetails, DockerEngine, Metadata},
};
use std::{collections::BTreeMap, fs, os::unix::fs::symlink};

#[test]
fn collected_optional_data_reaches_the_correct_prometheus_families_and_labels() {
    let temp = common::fixture();
    let root = temp.path();
    common::v2(root, &format!("docker/{}", common::ID), 1_000_000);
    for tid in [123, 124] {
        common::put(
            root,
            &format!("proc/123/task/{tid}/stat"),
            &format!("{tid} (worker) S 0\n"),
        );
        common::put(
            root,
            &format!("proc/123/task/{tid}/schedstat"),
            "1000000000 50000000 7\n",
        );
    }
    fs::create_dir_all(root.join("proc/123/fd")).unwrap();
    symlink("socket:[1]", root.join("proc/123/fd/0")).unwrap();
    common::put(
        root,
        "proc/123/limits",
        "Max open files            1024                 4096                 files\n",
    );
    fs::create_dir_all(root.join("proc/123/root/data")).unwrap();
    let header = "sl local_address rem_address st queues timer retrans uid timeout inode\n";
    let tcp = format!(
        "{header}0: 00000000:0050 00000000:0000 0A 00000000:00000000 00:00000000 0 0 0 123 1 0\n"
    );
    let udp = format!(
        "{header}0: 00000000:0035 00000000:0000 07 00000020:00000010 00:00000000 0 1000 0 12345 2 0000000000000000 9\n"
    );
    for name in ["tcp", "tcp6"] {
        common::put(root, &format!("proc/123/net/{name}"), &tcp);
    }
    for name in ["udp", "udp6"] {
        common::put(root, &format!("proc/123/net/{name}"), &udp);
    }
    let metadata = BTreeMap::from([(
        common::ID.into(),
        Metadata {
            name: "web".into(),
            rootfs_bytes: Some(8192),
            writable_bytes: Some(4096),
            state: Some(ContainerState::Running),
            details: DockerDetails {
                pid: Some(123),
                running: Some(true),
                started: Some(1_735_787_045.0),
                restarts: Some(3),
                exit_code: Some(0),
                oom_killed: Some(false),
                restart_policy: Some("on-failure".into()),
                restart_maximum: Some(5),
                healthcheck_configured: Some(true),
                mounts: vec!["/data".into()],
                ..DockerDetails::default()
            },
            ..Metadata::default()
        },
    )]);
    let mut sample = Collector::new(common::config(root))
        .unwrap()
        .collect(&metadata, true)
        .unwrap();
    sample.docker_engine = Some(DockerEngine {
        containers: BTreeMap::from([("running".into(), 1), ("stopped".into(), 2)]),
        images: Some(3),
        files: Some(4),
        goroutines: Some(5),
        event_listeners: Some(6),
        cpu_cores: Some(8),
        memory: Some(1024),
        version: "29\"quoted\\\n".into(),
        driver: "overlayfs".into(),
        os: "linux".into(),
        arch: "x86_64".into(),
        cgroup_version: "2".into(),
        cgroup_driver: "systemd".into(),
    });
    let text = String::from_utf8(metrics::encode(&sample, &Labels::default()).to_vec()).unwrap();
    for (name, value) in [
        ("container_processes", "1"),
        ("container_file_descriptors", "1"),
        ("container_sockets", "1"),
        ("container_cpu_schedstat_run_seconds_total", "2"),
        ("container_cpu_schedstat_run_periods_total", "14"),
        ("container_restarts_total", "3"),
        ("container_fs_rootfs_size_bytes", "8192"),
        ("container_restart_maximum_retry_count", "5"),
        ("container_last_exit_code", "0"),
        ("container_oom_killed", "0"),
        ("container_pid", "123"),
        ("container_healthcheck_configured", "1"),
    ] {
        assert!(
            text.lines()
                .any(|line| line.starts_with(&format!("{name}{{"))
                    && line.ends_with(&format!("}} {value}"))),
            "missing {name}"
        );
    }
    assert!(text.contains("state=\"sleeping\"} 2\n"));
    assert!(text.contains("tcp_state=\"listen\"} 1\n"));
    assert!(text.contains("udp_state=\"rxqueued\"} 16\n"));
    assert!(text.contains("ulimit=\"max_open_files\"} 1024\n"));
    assert!(text.contains("ulimit=\"max_open_files\"} 4096\n"));
    assert!(text.contains("device=\"rootfs\"} 4096\n"));
    assert!(text.contains("policy=\"on-failure\"} 1\n"));
    assert!(text.contains("# TYPE container_network_tcp_usage_total gauge\n"));
    assert!(text.contains("docker_engine_containers{state=\"stopped\"} 2\n"));
    assert!(text.contains("docker_engine_images 3\n"));
    assert!(text.contains("version=\"29\\\"quoted\\\\\\n\""));
    let mut types = std::collections::BTreeSet::new();
    for line in text.lines().filter(|line| line.starts_with("# TYPE ")) {
        assert!(
            types.insert(line.split_whitespace().nth(2).unwrap()),
            "duplicate family declaration"
        );
    }
}
