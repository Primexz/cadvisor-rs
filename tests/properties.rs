//! Properties for untrusted kernel text and deterministic counter conversions.

mod common;

use cadvisor_rs::{cgroup, collector::Collector, metrics::Labels, procfs};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0xcad1_5001),
        ..ProptestConfig::default()
    })]
    #[test]
    fn parsers_accept_arbitrary_utf8_without_panicking(text in any::<String>()) {
        let _ = cgroup::key_values(&text);
        let _ = cgroup::parse_io_v2(&text);
        let _ = cgroup::container_id(&text);
        let _ = cgroup::limit(&text);
        let _ = procfs::parse_network(&text);
        let _ = cgroup::cpuset_count(&text);
        let _ = cgroup::parse_pressure(&text);
        let _ = procfs::parse_tcp(&text);
        let _ = procfs::parse_udp(&text);
        let _ = procfs::parse_ulimits(&text);
    }

    #[test]
    fn finite_limits_round_trip(value in 0u64..(1u64 << 60)) {
        prop_assert_eq!(cgroup::limit(&value.to_string()), Some(value));
    }

    #[test]
    fn cpu_nanoseconds_and_microseconds_agree(microseconds in 0u64..1_000_000_000_000) {
        let a = common::fixture();
        let b = common::fixture();
        common::v1(a.path(), "test", microseconds * 1000);
        common::v2(b.path(), "test", microseconds);
        let a = Collector::new(common::config(a.path())).unwrap().collect(&Default::default(), false).unwrap();
        let b = Collector::new(common::config(b.path())).unwrap().collect(&Default::default(), false).unwrap();
        prop_assert!((a.containers[0].cpu.usage.unwrap() - b.containers[0].cpu.usage.unwrap()).abs() <= 1e-9);
    }
}

#[test]
fn label_collisions_and_duplicate_keys_are_rejected() {
    assert!(Labels::new(&["app.role".into(), "app-role".into()]).is_err());
    assert!(Labels::new(&["app".into(), "app".into()]).is_err());
    assert!(Labels::new(&["".into()]).is_err());
    assert!(Labels::new(&["com.docker.compose.service".into()]).is_ok());
}

#[test]
fn runtime_ids_require_a_complete_component() {
    let id = common::ID;
    assert_eq!(cgroup::container_id(&format!("/docker/{id}")), Some(id));
    assert_eq!(
        cgroup::container_id(&format!("/system.slice/docker-{id}.scope")),
        Some(id)
    );
    assert_eq!(
        cgroup::container_id(&format!("/kubepods/cri-containerd-{id}.scope")),
        Some(id)
    );
    assert_eq!(cgroup::container_id(&format!("/crio-{id}.scope")), Some(id));
    assert_eq!(cgroup::container_id(&format!("/prefix{id}suffix")), None);
}
