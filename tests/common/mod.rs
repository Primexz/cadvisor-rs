//! Synthetic Linux controller mounts used by integration tests.
#![allow(dead_code)]

#[cfg(unix)]
pub mod http;

use std::{fs, path::Path};

use cadvisor_rs::config::Config;
use clap::Parser;
use tempfile::TempDir;

pub const ID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

pub fn put(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

pub fn fixture() -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    put(
        temp.path(),
        "proc/meminfo",
        "MemTotal: 8192 kB\nSwapTotal: 1024 kB\n",
    );
    put(
        temp.path(),
        "proc/stat",
        "cpu 1 2 3\ncpu0 1 2 3\ncpu1 4 5 6\nintr 1\n",
    );
    temp
}

pub fn config(root: &Path) -> Config {
    Config::parse_from([
        "cadvisor-rs",
        "--disable-docker",
        "--cgroup-root",
        root.join("cgroup").to_str().unwrap(),
        "--proc-root",
        root.join("proc").to_str().unwrap(),
    ])
}

pub fn v2(root: &Path, relative: &str, usage: u64) {
    let group = root.join("cgroup").join(relative);
    put(root, "cgroup/cgroup.controllers", "cpu memory io pids\n");
    put(
        &group,
        "cpu.stat",
        &format!(
            "usage_usec {usage}\nuser_usec 2000000\nsystem_usec 1000000\nnr_periods 50\nnr_throttled 4\nthrottled_usec 125000\n"
        ),
    );
    put(&group, "cpu.max", "200000 100000\n");
    put(&group, "memory.current", "4096\n");
    put(
        &group,
        "memory.stat",
        "anon 1024\nfile 3072\ninactive_file 2048\n",
    );
    put(&group, "memory.max", "max\n");
    put(&group, "memory.swap.current", "128\n");
    put(&group, "memory.events", "max 3\noom 2\noom_kill 1\n");
    put(&group, "pids.current", "7\n");
    put(&group, "pids.max", "100\n");
    put(
        &group,
        "io.stat",
        "8:0 rbytes=4096 wbytes=8192 rios=2 wios=4 dbytes=0 dios=0\n",
    );
    put(&group, "cgroup.procs", "123\n");
    put(
        root,
        "proc/123/net/dev",
        "Inter-| Receive | Transmit\n eth0: 100 2 3 4 0 0 0 0 200 6 7 8 0 0 0 0\n lo: 10 1 0 0 0 0 0 0 10 1 0 0 0 0 0 0\n",
    );
}

pub fn v1(root: &Path, relative: &str, usage_ns: u64) {
    let put_controller = |controller: &str, file: &str, text: &str| {
        put(
            &root.join("cgroup").join(controller).join(relative),
            file,
            text,
        );
    };
    put_controller("cpu,cpuacct", "cpuacct.usage", &usage_ns.to_string());
    put_controller("cpu,cpuacct", "cpuacct.usage_user", "2000000000\n");
    put_controller("cpu,cpuacct", "cpuacct.usage_sys", "1000000000\n");
    put_controller(
        "cpu,cpuacct",
        "cpu.stat",
        "nr_periods 50\nnr_throttled 4\nthrottled_time 125000000\n",
    );
    put_controller("cpu,cpuacct", "cpu.cfs_quota_us", "200000\n");
    put_controller("cpu,cpuacct", "cpu.cfs_period_us", "100000\n");
    put_controller("memory", "memory.usage_in_bytes", "4096\n");
    put_controller(
        "memory",
        "memory.stat",
        "rss 999\ntotal_rss 1024\ntotal_cache 3072\ntotal_inactive_file 2048\ntotal_swap 128\n",
    );
    put_controller("memory", "memory.limit_in_bytes", "9223372036854771712\n");
    put_controller("memory", "memory.failcnt", "3\n");
    put_controller("memory", "cgroup.procs", "123\n");
    put_controller("pids", "pids.current", "7\n");
    put_controller("pids", "pids.max", "100\n");
    put_controller(
        "blkio",
        "blkio.throttle.io_service_bytes",
        "8:0 Read 4096\n8:0 Write 8192\n8:0 Total 12288\nTotal 12288\n",
    );
    put_controller(
        "blkio",
        "blkio.throttle.io_serviced",
        "8:0 Read 2\n8:0 Write 4\n",
    );
}
