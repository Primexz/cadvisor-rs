//! Real filesystem fixtures for task/FD limits, socket tables and capacity.
#![cfg(unix)]
mod common;

use cadvisor_rs::procfs;
use common::put;
use std::{fs, os::unix::fs::symlink};

fn task(root: &std::path::Path, pid: u32, tid: u32, state: &str, run: u64, wait: u64) {
    put(
        root,
        &format!("{pid}/task/{tid}/stat"),
        &format!("{tid} (a tricky ) name) {state} 0 0 0\n"),
    );
    put(
        root,
        &format!("{pid}/task/{tid}/schedstat"),
        &format!("{run} {wait} 9\n"),
    );
}

#[test]
fn processes_include_threads_socket_descriptors_schedstats_and_init_limits() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    task(root, 123, 123, "S", 1_500_000_000, 25_000_000);
    task(root, 123, 124, "R", 2_500_000_000, 75_000_000);
    task(root, 125, 125, "D", 0, 0);
    fs::create_dir_all(root.join("123/fd")).unwrap();
    fs::create_dir_all(root.join("125/fd")).unwrap();
    symlink("socket:[123]", root.join("123/fd/0")).unwrap();
    symlink("/dev/null", root.join("123/fd/1")).unwrap();
    symlink("pipe:[9]", root.join("125/fd/0")).unwrap();
    put(
        root,
        "125/limits",
        "Max open files            1024                 4096                 files\nMax cpu time unlimited unlimited seconds\n",
    );
    let value = procfs::read_processes(root, &[123, 125], Some(125), 64);
    assert_eq!(value.count, Some(2));
    assert_eq!(value.threads, Some(3));
    assert_eq!(value.files, Some(3));
    assert_eq!(value.sockets, Some(1));
    assert_eq!(value.tasks, Some([1, 1, 0, 1, 0, 0, 0]));
    assert_eq!(value.sched_runtime, Some(4.0));
    assert_eq!(value.sched_wait, Some(0.1));
    assert_eq!(value.sched_periods, Some(27));
    assert_eq!(value.errors, 0);
    let open = value
        .ulimits
        .iter()
        .find(|v| v.name == "max_open_files")
        .unwrap();
    assert_eq!((open.soft, open.hard), (1024, 4096));
    assert_eq!(
        value
            .ulimits
            .iter()
            .find(|v| v.name == "max_cpu_time")
            .unwrap()
            .soft,
        -1
    );
}

#[test]
fn incomplete_or_bounded_process_samples_omit_aggregates_instead_of_undercounting() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    task(root, 1, 1, "S", 1, 1);
    task(root, 1, 2, "Z", 1, 1);
    fs::create_dir_all(root.join("1/fd")).unwrap();
    let bounded = procfs::read_processes(root, &[1], None, 1);
    assert!(bounded.errors > 0);
    assert_eq!(bounded.threads, None);
    assert_eq!(bounded.tasks, None);
    assert_eq!(bounded.files, None);
    assert_eq!(bounded.sched_runtime, None);
    let missing = procfs::read_processes(root, &[1, 999], None, 64);
    assert!(missing.errors > 0);
    assert_eq!(missing.count, Some(2));
    assert_eq!(missing.files, None);
    assert_eq!(missing.tasks, None);
    assert_eq!(missing.sched_runtime, None);
    let empty = procfs::read_processes(root, &[], None, 64);
    assert_eq!(empty.count, Some(0));
    assert_eq!(empty.files, Some(0));
    assert_eq!(empty.tasks, Some([0; 7]));
}

#[test]
fn cgroup_subtree_membership_is_deduplicated_and_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    put(root, "a/cgroup.procs", "123\n125\n123\n");
    put(root, "b/cgroup.procs", "125\n126\n");
    let paths = [root.join("a"), root.join("b")];
    assert_eq!(procfs::pids(&paths, 3).unwrap(), [123, 125, 126]);
    assert!(procfs::pids(&paths, 2).is_err());
    assert!(procfs::pids(&[root.join("missing")], 3).is_err());
    put(root, "a/cgroup.procs", "malformed\n");
    assert!(procfs::pids(&paths, 3).is_err());
}

#[test]
fn tcp_udp_tables_preserve_states_queues_and_missing_ipv6() {
    let header = "sl local_address rem_address st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode\n";
    let established =
        "0: 0100007F:1234 0100007F:4321 01 00000000:00000000 00:00000000 0 0 0 123 1 0\n";
    let listen = "1: 00000000:0050 00000000:0000 0A 00000000:00000000 00:00000000 0 0 0 124 1 0\n";
    let udp = "0: 00000000:0035 00000000:0000 07 00000020:00000010 00:00000000 0 1000 0 12345 2 0000000000000000 9\n";
    let text = format!("{header}{established}{listen}");
    let tcp = procfs::parse_tcp(&text).unwrap();
    assert_eq!(tcp[0], 1);
    assert_eq!(tcp[9], 1);
    assert_eq!(
        procfs::parse_udp(&format!("{header}{udp}")),
        Some([1, 9, 16, 32])
    );
    assert_eq!(procfs::parse_tcp("malformed"), None);
    assert_eq!(procfs::parse_udp("bad"), None);
    let temp = tempfile::tempdir().unwrap();
    put(temp.path(), "123/net/tcp", &text);
    put(temp.path(), "123/net/udp", &format!("{header}{udp}"));
    let value = procfs::read_connections(temp.path(), Some(123), false);
    assert_eq!(value.tcp, Some(tcp));
    assert_eq!(value.udp, Some([1, 9, 16, 32]));
    assert_eq!(value.tcp6, None);
    assert_eq!(value.errors, 0);
    put(temp.path(), "123/net/tcp6", "invalid");
    assert_eq!(
        procfs::read_connections(temp.path(), Some(123), false).errors,
        1
    );
}

#[test]
fn filesystem_capacity_never_becomes_container_usage_or_escapes_the_root() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("123/root/data")).unwrap();
    let (values, errors) = procfs::read_filesystems(
        temp.path(),
        Some(123),
        &["/data".into(), "/../escape".into()],
        Some(4096),
    );
    assert_eq!(errors, 0);
    assert_eq!(values.len(), 2);
    let root = values.iter().find(|v| v.device == "rootfs").unwrap();
    let mount = values.iter().find(|v| v.device == "/data").unwrap();
    assert!(root.limit.unwrap() > 0);
    assert!(root.inodes.is_some());
    assert_eq!(root.usage, Some(4096));
    assert_eq!(mount.usage, None);
    let (stopped, errors) = procfs::read_filesystems(temp.path(), None, &[], Some(0));
    assert_eq!(stopped[0].usage, Some(0));
    assert_eq!(stopped[0].limit, None);
    assert_eq!(errors, 0);
    let (missing, errors) = procfs::read_filesystems(temp.path(), Some(999), &[], None);
    assert!(missing.is_empty());
    assert_eq!(errors, 1);
}
