//! Additional resource metrics exercised through collection and Prometheus encoding.
mod common;

use cadvisor_rs::{
    cgroup,
    collector::Collector,
    config::Config,
    metrics::{self, Labels},
    procfs,
};
use common::{config, fixture, put, v1, v2};
use std::{collections::BTreeMap, path::Path};

fn scrape(config: Config) -> String {
    let sample = Collector::new(config)
        .unwrap()
        .collect(&BTreeMap::new(), false)
        .unwrap();
    assert_eq!(sample.errors, 0);
    String::from_utf8(metrics::encode(&sample, &Labels::default()).to_vec()).unwrap()
}

fn sample(text: &str, name: &str, labels: &[&str], value: &str) {
    assert!(
        text.lines().any(|line| {
            line.starts_with(&format!("{name}{{"))
                && labels.iter().all(|label| line.contains(label))
                && line.ends_with(&format!("}} {value}"))
        }),
        "missing {name} with {labels:?} = {value}"
    );
}

fn no_samples(text: &str, name: &str) {
    assert!(
        !text
            .lines()
            .any(|line| line.starts_with(&format!("{name}{{"))),
        "unexpected samples for {name}"
    );
}

#[test]
fn unified_hugetlb_numa_and_iocost_keep_units_and_availability() {
    let temp = fixture();
    let root = temp.path();
    v2(root, "work", 3_000_000);
    let group = root.join("cgroup/work");
    let page = rustix::param::page_size() as u64;
    put(
        &group,
        "memory.numa_stat",
        &format!(
            "file N3={} N0={}\nnew_stat N0=999\nanon N0={} N3={}\nunevictable N0=0 N3=0\n",
            page * 7,
            page * 3,
            page * 2,
            page * 5,
        ),
    );
    put(&group, "hugetlb.2MB.current", "2097152\n");
    put(&group, "hugetlb.2MB.max", "4194304\n");
    put(&group, "hugetlb.2MB.events", "max 3\n");
    put(&group, "hugetlb.1GB.current", "0\n");
    put(&group, "hugetlb.1GB.max", "max\n");
    put(&group, "hugetlb.1GB.events", "max 0\n");
    put(&group, "hugetlb.2MB.rsvd.current", "123\n");
    put(&group, "hugetlb.invalid.current", "123\n");
    put(
        &group,
        "io.stat",
        "8:0 rbytes=4096 wbytes=8192 rios=2 wios=4 cost.usage=2500000 cost.wait=1000000 cost.indebt=400000 cost.indelay=50000\n8:16 wbytes=4096 wios=1\n",
    );

    let text = scrape(config(root));
    sample(
        &text,
        "container_hugetlb_usage_bytes",
        &["pagesize=\"2MB\""],
        "2097152",
    );
    sample(
        &text,
        "container_hugetlb_failcnt",
        &["pagesize=\"2MB\""],
        "3",
    );
    sample(
        &text,
        "container_spec_hugetlb_limit_bytes",
        &["pagesize=\"2MB\""],
        "4194304",
    );
    sample(
        &text,
        "container_hugetlb_usage_bytes",
        &["pagesize=\"1GB\""],
        "0",
    );
    sample(
        &text,
        "container_hugetlb_failcnt",
        &["pagesize=\"1GB\""],
        "0",
    );
    no_samples(&text, "container_hugetlb_max_usage_bytes");
    assert!(!text.lines().any(
        |line| line.starts_with("container_spec_hugetlb_limit_bytes{")
            && line.contains("pagesize=\"1GB\"")
    ));
    assert!(!text.contains("pagesize=\"2MB.rsvd\""));
    assert!(!text.contains("pagesize=\"invalid\""));
    sample(
        &text,
        "container_memory_numa_pages",
        &["type=\"anon\"", "scope=\"hierarchy\"", "node=\"0\""],
        "2",
    );
    sample(
        &text,
        "container_memory_numa_pages",
        &["type=\"file\"", "scope=\"hierarchy\"", "node=\"3\""],
        "7",
    );
    assert!(
        !text
            .lines()
            .any(|line| line.starts_with("container_memory_numa_pages{")
                && line.contains("scope=\"container\""))
    );
    for (name, value) in [
        ("container_fs_io_cost_usage_seconds_total", "2.5"),
        ("container_fs_io_cost_wait_seconds_total", "1"),
        ("container_fs_io_cost_indebt_seconds_total", "0.4"),
        ("container_fs_io_cost_indelay_seconds_total", "0.05"),
    ] {
        sample(&text, name, &["device=\"8:0\""], value);
        assert!(
            !text
                .lines()
                .any(|line| line.starts_with(&format!("{name}{{"))
                    && line.contains("device=\"8:16\""))
        );
    }
    no_samples(&text, "container_fs_read_seconds_total");
    no_samples(&text, "container_memory_migrate");
}

#[test]
fn legacy_hugetlb_numa_and_block_scheduler_details_are_exposed() {
    let temp = fixture();
    let root = temp.path();
    v1(root, "work", 3_000_000_000);
    let huge = root.join("cgroup/hugetlb/work");
    for (file, value) in [
        ("hugetlb.64KB.usage_in_bytes", "65536"),
        ("hugetlb.64KB.max_usage_in_bytes", "131072"),
        ("hugetlb.64KB.limit_in_bytes", "262144"),
        ("hugetlb.64KB.failcnt", "2"),
        ("hugetlb.64KB.rsvd.usage_in_bytes", "999999"),
    ] {
        put(&huge, file, value);
    }
    put(
        &root.join("cgroup/memory/work"),
        "memory.numa_stat",
        "total=12 N0=5 N1=7\nanon=10 N0=4 N1=6\nfile=2 N0=1 N1=1\nunevictable=0 N0=0 N1=0\nhierarchical_anon=20 N0=9 N1=11\nhierarchical_file=4 N0=2 N1=2\n",
    );
    put(
        &root.join("cgroup/cpuset/work"),
        "cpuset.memory_migrate",
        "1\n",
    );
    let io = root.join("cgroup/blkio/work");
    put(
        &io,
        "blkio.io_service_time_recursive",
        "8:0 Read 1000000000\n8:0 Write 250000000\n8:0 Sync 1250000000\n8:0 Async 0\n8:0 Total 1250000000\nTotal 1250000000\n",
    );
    put(&io, "blkio.io_service_time", "8:0 Read 99999999999\n");
    put(
        &io,
        "blkio.io_merged_recursive",
        "8:0 Read 2\n8:0 Write 3\n8:0 Total 5\n",
    );
    put(
        &io,
        "blkio.io_queued_recursive",
        "8:0 Read 1\n8:0 Write 3\n8:0 Sync 4\n8:0 Async 0\n8:0 Total 4\n",
    );

    let text = scrape(config(root));
    sample(
        &text,
        "container_hugetlb_usage_bytes",
        &["pagesize=\"64KB\""],
        "65536",
    );
    sample(
        &text,
        "container_hugetlb_max_usage_bytes",
        &["pagesize=\"64KB\""],
        "131072",
    );
    sample(
        &text,
        "container_hugetlb_failcnt",
        &["pagesize=\"64KB\""],
        "2",
    );
    sample(
        &text,
        "container_memory_numa_pages",
        &["type=\"anon\"", "scope=\"container\"", "node=\"1\""],
        "6",
    );
    sample(
        &text,
        "container_memory_numa_pages",
        &["type=\"anon\"", "scope=\"hierarchy\"", "node=\"1\""],
        "11",
    );
    sample(&text, "container_memory_migrate", &[], "1");
    for (name, value) in [
        ("container_fs_read_seconds_total", "1"),
        ("container_fs_write_seconds_total", "0.25"),
        ("container_fs_io_time_seconds_total", "1.25"),
        ("container_fs_reads_merged_total", "2"),
        ("container_fs_writes_merged_total", "3"),
        ("container_fs_io_current", "4"),
    ] {
        sample(&text, name, &["device=\"8:0\""], value);
    }
    assert!(text.contains("# TYPE container_fs_io_current gauge\n"));
    no_samples(&text, "container_fs_io_time_weighted_seconds_total");
}

fn tcp_files(root: &Path) {
    put(
        root,
        "proc/123/net/snmp",
        "Ip: Forwarding DefaultTTL\nIp: 2 64\nTcp: OutSegs Unknown RetransSegs MaxConn ActiveOpens CurrEstab\nTcp: 18446744073709551615 999 23 -1 7 2\nUdp: InDatagrams\nUdp: 100\n",
    );
    put(
        root,
        "proc/123/net/netstat",
        "TcpExt: ListenDrops TCPTimeouts ListenOverflows TCPSynRetrans FutureCounter\nTcpExt: 4 5 6 7 999\nIpExt: InOctets\nIpExt: 1024\n",
    );
    put(root, "proc/net/snmp", "Tcp: RetransSegs\nTcp: 999999\n");
}

#[test]
fn advanced_tcp_uses_the_container_namespace_and_preserves_counter_values() {
    let temp = fixture();
    let root = temp.path();
    v2(root, "work", 3_000_000);
    tcp_files(root);
    let text = scrape(config(root));
    for (state, value) in [
        ("outsegs", "18446744073709551615"),
        ("retranssegs", "23"),
        ("maxconn", "-1"),
        ("activeopens", "7"),
        ("currestab", "2"),
        ("listendrops", "4"),
        ("tcptimeouts", "5"),
        ("listenoverflows", "6"),
        ("tcpsynretrans", "7"),
    ] {
        sample(
            &text,
            "container_network_advance_tcp_stats_total",
            &[&format!("tcp_state=\"{state}\"")],
            value,
        );
    }
    assert!(!text.contains("tcp_state=\"unknown\""));
    assert!(!text.contains("tcp_state=\"futurecounter\""));
    assert!(text.contains("# TYPE container_network_advance_tcp_stats_total gauge\n"));
    let mut cfg = config(root);
    cfg.disable_network_connections = true;
    no_samples(&scrape(cfg), "container_network_advance_tcp_stats_total");
}

#[test]
fn unsupported_resource_interfaces_do_not_produce_zero_measurements() {
    for legacy in [false, true] {
        let temp = fixture();
        if legacy {
            v1(temp.path(), "work", 1);
        } else {
            v2(temp.path(), "work", 1);
        }
        let text = scrape(config(temp.path()));
        for name in [
            "container_hugetlb_usage_bytes",
            "container_hugetlb_max_usage_bytes",
            "container_hugetlb_failcnt",
            "container_spec_hugetlb_limit_bytes",
            "container_memory_numa_pages",
            "container_memory_migrate",
            "container_network_advance_tcp_stats_total",
            "container_fs_read_seconds_total",
            "container_fs_io_current",
            "container_fs_io_cost_usage_seconds_total",
        ] {
            no_samples(&text, name);
        }
        sample(
            &text,
            "container_cpu_usage_seconds_total",
            &["cpu=\"total\""],
            if legacy { "0.000000001" } else { "0.000001" },
        );
    }
}

#[test]
fn numa_conversion_supports_4k_16k_and_64k_base_pages() {
    for page_size in [4096, 16384, 65536] {
        let text = format!(
            "file N0={} N2={}\nanon N0={}\n",
            page_size * 7,
            page_size * 3,
            page_size * 2
        );
        let stats = cgroup::parse_numa_v2(&text, page_size);
        assert_eq!(stats.hierarchy[&0].file, Some(7));
        assert_eq!(stats.hierarchy[&2].file, Some(3));
        assert_eq!(stats.hierarchy[&0].anon, Some(2));
        assert!(stats.container.is_empty());
    }
    let stats = cgroup::parse_numa_v2("file N0=4096\n", 0);
    assert!(stats.hierarchy.is_empty());
    let stats = cgroup::parse_numa_v1(
        "anon=7 N0=7\nhierarchical_anon=9 N0=9\nfile=bad N0=5\nunevictable=0 N0=0\n",
    );
    assert_eq!(stats.container[&0].anon, Some(7));
    assert_eq!(stats.hierarchy[&0].anon, Some(9));
    assert_eq!(stats.container[&0].file, None);
    assert_eq!(stats.container[&0].unevictable, Some(0));
}

#[test]
fn bfq_fallbacks_preserve_discard_counts_and_do_not_double_count_operation_classes() {
    let temp = fixture();
    let root = temp.path();
    v1(root, "work", 1);
    let path = root.join("cgroup/blkio/work");
    std::fs::remove_file(path.join("blkio.throttle.io_service_bytes")).unwrap();
    std::fs::remove_file(path.join("blkio.throttle.io_serviced")).unwrap();
    put(
        &path,
        "blkio.bfq.io_service_bytes_recursive",
        "8:16 Read 4096\n8:16 Write 8192\n8:16 Discard 16384\n8:16 Sync 12288\n8:16 Async 0\n8:16 Total 28672\nTotal 28672\nbad Read 100\n",
    );
    put(
        &path,
        "blkio.bfq.io_serviced_recursive",
        "8:16 Read 1\n8:16 Write 2\n8:16 Discard 4\n",
    );
    put(
        &path,
        "blkio.bfq.io_service_time",
        "8:16 Read 750000000\n8:16 Total 750000000\n",
    );
    put(
        &path,
        "blkio.bfq.io_merged",
        "8:16 Read 2\n8:16 Write 0\n8:16 Sync 2\n",
    );
    put(
        &path,
        "blkio.bfq.io_queued",
        "8:16 Read 1\n8:16 Write 0\n8:16 Sync 1\n8:16 Total 1\n",
    );
    let text = scrape(config(root));
    for (name, value) in [
        ("container_fs_reads_bytes_total", "4096"),
        ("container_fs_writes_bytes_total", "8192"),
        ("container_fs_discards_bytes_total", "16384"),
        ("container_fs_discards_total", "4"),
        ("container_fs_read_seconds_total", "0.75"),
        ("container_fs_io_time_seconds_total", "0.75"),
        ("container_fs_reads_merged_total", "2"),
        ("container_fs_io_current", "1"),
    ] {
        sample(&text, name, &["device=\"8:16\""], value);
    }
    no_samples(&text, "container_fs_write_seconds_total");
    assert!(!text.contains("device=\"bad\""));
    assert!(!text.contains("device=\"Total\""));
}

#[test]
fn malformed_tcp_groups_are_omitted_without_hiding_valid_protocols() {
    for text in [
        "Tcp: RetransSegs OutSegs\nTcp: 1\n",
        "Tcp: RetransSegs\nTcp: -1\n",
        "Tcp: RetransSegs\nTcp: invalid\n",
        "Tcp: MaxConn\nTcp: -2\n",
        "Tcp: RetransSegs RetransSegs\nTcp: 1 2\n",
        "Tcp: MaxConn MaxConn\nTcp: -1 -1\n",
        "Tcp: RetransSegs\nUdp: 1\n",
        "Tcp: RetransSegs\nTcp: 1 2\n",
        "Tcp: RetransSegs\nTcp: 1\nTcp: RetransSegs\nTcp: 2\n",
    ] {
        assert!(
            procfs::parse_advanced_tcp(text, "Tcp").is_none(),
            "accepted malformed group: {text}"
        );
    }
    let temp = fixture();
    let root = temp.path();
    v2(root, "work", 1);
    tcp_files(root);
    put(
        root,
        "proc/123/net/snmp",
        "Tcp: RetransSegs OutSegs\nTcp: 1\n",
    );
    let stats = procfs::read_connections(&root.join("proc"), Some(123), false);
    assert_eq!(stats.errors, 1);
    assert!(!stats.advanced.values.contains_key("retranssegs"));
    assert_eq!(stats.advanced.values["tcptimeouts"], 5);
    let text = scrape(config(root));
    sample(
        &text,
        "container_network_advance_tcp_stats_total",
        &["tcp_state=\"tcptimeouts\""],
        "5",
    );
    assert!(!text.contains("tcp_state=\"retranssegs\""));
}

#[test]
fn iocost_ignores_unknown_devices_and_rejects_invalid_or_negative_values() {
    let values = cgroup::parse_io_v2(
        "8:0 cost.usage=2000000\n8:16 future.counter=1\n8:32 cost.wait=-1\n8:48 cost.indelay=NaN\nbad cost.usage=2000000\n",
    );
    assert_eq!(values.len(), 1);
    assert_eq!(values["8:0"].cost_usage_seconds, Some(2.0));
    assert_eq!(values["8:0"].reads, None);
}

#[test]
fn json_snapshot_preserves_numa_pages_and_unsigned_tcp_counters() {
    let temp = fixture();
    let root = temp.path();
    v2(root, "work", 1);
    tcp_files(root);
    put(
        &root.join("cgroup/work"),
        "memory.numa_stat",
        &format!("anon N0={}\n", rustix::param::page_size() * 3),
    );
    let collected = Collector::new(config(root))
        .unwrap()
        .collect(&BTreeMap::new(), false)
        .unwrap();
    let json = serde_json::to_value(collected).unwrap();
    assert_eq!(
        json["containers"][0]["memory"]["details"]["numa"]["hierarchy"]["0"]["anon"],
        3
    );
    assert_eq!(
        json["containers"][0]["connections"]["advanced"]["values"]["outsegs"].as_u64(),
        Some(u64::MAX)
    );
    assert_eq!(
        json["containers"][0]["connections"]["advanced"]["max_connections"],
        -1
    );
}
