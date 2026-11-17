//! Health observations must distinguish no check, failed checks and stale data.

mod common;

use std::collections::BTreeMap;

use cadvisor_rs::{
    collector::Collector,
    metrics::{self, Labels},
    model::{ContainerState, HealthStatus, Metadata},
};

#[test]
fn health_transitions_and_no_healthcheck_have_distinct_metric_labels() {
    let temp = common::fixture();
    common::v2(temp.path(), &format!("docker/{}", common::ID), 3_000_000);
    let mut collector = Collector::new(common::config(temp.path())).unwrap();
    let mut metadata = BTreeMap::from([(
        common::ID.into(),
        Metadata {
            name: "web".into(),
            health: Some(HealthStatus::Healthy),
            health_failing_streak: Some(0),
            state: Some(ContainerState::Running),
            ..Metadata::default()
        },
    )]);
    let sample = collector.collect(&metadata, true).unwrap();
    let text = String::from_utf8(metrics::encode(&sample, &Labels::default()).to_vec()).unwrap();
    assert!(text.contains("status=\"healthy\"} 1\n"));
    assert!(text.contains("state=\"running\"} 1\n"));
    assert!(text.contains("container_health_failing_streak{"));
    assert!(text.contains("cpu=\"total\"} 3\n"));
    assert!(text.contains("container_memory_usage_bytes{"));

    let entry = metadata.get_mut(common::ID).unwrap();
    entry.health = Some(HealthStatus::Unhealthy);
    entry.health_failing_streak = Some(4);
    let sample = collector.collect(&metadata, true).unwrap();
    let text = String::from_utf8(metrics::encode(&sample, &Labels::default()).to_vec()).unwrap();
    assert!(text.contains("status=\"unhealthy\"} 1\n"));
    assert!(!text.contains("status=\"healthy\""));
    assert!(
        text.lines()
            .any(|line| line.starts_with("container_health_failing_streak{")
                && line.ends_with("} 4"))
    );

    let entry = metadata.get_mut(common::ID).unwrap();
    entry.health = Some(HealthStatus::NoHealthcheck);
    entry.health_failing_streak = None;
    let sample = collector.collect(&metadata, true).unwrap();
    let text = String::from_utf8(metrics::encode(&sample, &Labels::default()).to_vec()).unwrap();
    assert!(text.contains("status=\"none\"} 1\n"));
    assert!(
        !text
            .lines()
            .any(|line| line.starts_with("container_health_failing_streak{"))
    );
    assert!(!text.contains("status=\"healthy\""));
    let json = serde_json::to_value(&sample).unwrap();
    assert_eq!(json["containers"][0]["metadata"]["health"], "none");
}

#[test]
fn metadata_failure_suppresses_health_and_state_but_retains_resource_metrics() {
    let temp = common::fixture();
    common::v2(temp.path(), &format!("docker/{}", common::ID), 1);
    let metadata = BTreeMap::from([(
        common::ID.into(),
        Metadata {
            health: Some(HealthStatus::Healthy),
            health_failing_streak: Some(0),
            state: Some(ContainerState::Running),
            ..Metadata::default()
        },
    )]);
    let mut collector = Collector::new(common::config(temp.path())).unwrap();
    let sample = collector.collect(&metadata, false).unwrap();
    let text = String::from_utf8(metrics::encode(&sample, &Labels::default()).to_vec()).unwrap();
    assert!(!text.contains("container_health_"));
    assert!(!text.contains("container_state{"));
    assert!(text.contains("cadvisor_rs_docker_up 0\n"));
    assert!(text.contains("container_cpu_usage_seconds_total{"));
    assert!(text.contains("container_memory_usage_bytes{"));

    let sample = collector.collect(&BTreeMap::new(), true).unwrap();
    let text = String::from_utf8(metrics::encode(&sample, &Labels::default()).to_vec()).unwrap();
    assert!(
        !text
            .lines()
            .any(|line| line.starts_with("container_health_status{"))
    );
    assert!(
        !text
            .lines()
            .any(|line| line.starts_with("container_state{"))
    );
}
