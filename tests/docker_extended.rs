//! Full Docker list/inspect/info behavior with bounded concurrent Unix HTTP.
#![cfg(unix)]
mod common;

use cadvisor_rs::{
    docker::{DockerClient, DockerOptions},
    model::{ContainerState, HealthStatus},
};
use common::http::Server;
use serde_json::json;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[tokio::test]
async fn inspect_adds_restart_exit_timestamps_and_limits_without_exporting_secrets() {
    let temp = tempfile::tempdir().unwrap();
    let ids: Vec<_> = (0..12).map(|i| format!("{i:064x}")).collect();
    let list: Vec<_> = ids.iter().map(|id| json!({"Id":id,"Names":["/web"],"State":"running","SizeRw":4096,"SizeRootFs":8192})).collect();
    let inspect = json!({
        "State":{"Status":"running","Running":true,"Pid":42,"ExitCode":0,"OOMKilled":false,
          "StartedAt":"2025-01-02T03:04:05.123456789Z","FinishedAt":"0001-01-01T00:00:00Z",
          "Health":{"Status":"healthy","FailingStreak":0,"Log":[{"Output":"private application output"}]}},
        "RestartCount":3,"Config":{"Env":["TOKEN=do-not-export"],"Healthcheck":{"Test":["CMD-SHELL","do-not-export command"]}},
        "HostConfig":{"Memory":1048576,"MemoryReservation":524288,"MemorySwap":2097152,
          "CpuShares":128,"CpuQuota":50000,"CpuPeriod":100000,"NanoCpus":500000000,"PidsLimit":64,
          "RestartPolicy":{"Name":"on-failure","MaximumRetryCount":5}},
        "Mounts":[{"Destination":"/data","Source":"/secret/host/path"},{"Destination":"/../escape"}]
    });
    let mut routes = BTreeMap::from([(
        "/containers/json?all=true&size=true".into(),
        (200, serde_json::to_string(&list).unwrap()),
    )]);
    for id in &ids {
        routes.insert(format!("/containers/{id}/json"), (200, inspect.to_string()));
    }
    let server = Server::new(temp.path(), routes, Duration::from_millis(25));
    let client = DockerClient::with_options(
        &server.socket,
        vec![],
        DockerOptions {
            sizes: true,
            ..DockerOptions::default()
        },
    )
    .unwrap();
    let metadata = client.refresh().await.unwrap();
    assert_eq!(metadata.len(), 12);
    let item = &metadata[&ids[0]];
    assert_eq!(item.inspect_up, Some(true));
    assert_eq!(item.runtime_id, ids[0]);
    assert_eq!(item.details.pid, Some(42));
    assert_eq!(item.details.restarts, Some(3));
    assert!((item.details.started.unwrap() - 1_735_787_045.123_456_7).abs() < 1e-6);
    assert_eq!(item.details.finished, None);
    assert_eq!(item.details.running, Some(true));
    assert_eq!(item.details.healthcheck_configured, Some(true));
    assert_eq!(item.details.memory, Some(1048576));
    assert_eq!(item.details.memory_swap, Some(2097152));
    assert_eq!(item.details.cpu_shares, Some(128));
    assert_eq!(item.details.cpu_quota, Some(50000));
    assert_eq!(item.details.nano_cpus, Some(500000000));
    assert_eq!(item.details.pids_limit, Some(64));
    assert_eq!(item.details.restart_policy.as_deref(), Some("on-failure"));
    assert_eq!(item.details.mounts, ["/data"]);
    assert_eq!(item.health, Some(HealthStatus::Healthy));
    assert_eq!(item.writable_bytes, Some(4096));
    let encoded = serde_json::to_string(item).unwrap();
    assert!(!encoded.contains("do-not-export"));
    assert!(!encoded.contains("private application"));
    assert!(!encoded.contains("secret/host"));
    assert!(server.peak() <= 8 && server.peak() >= 2);
    assert_eq!(server.requests().len(), 13);
}

#[tokio::test]
async fn stopped_inventory_and_inspect_failures_do_not_invalidate_docker_list() {
    let temp = tempfile::tempdir().unwrap();
    let a = format!("{:064x}", 1);
    let b = format!("{:064x}", 2);
    let list = json!([{"Id":a,"State":"exited","Status":"Exited (7)","Names":["/stopped"]},
        {"Id":b,"State":"running","Status":"Up 1 second (healthy)"}]);
    let server=Server::new(temp.path(),BTreeMap::from([
        ("/containers/json?all=true".into(),(200,list.to_string())),
        (format!("/containers/{a}/json"),(200,json!({"State":{"Status":"exited","Running":false,"Pid":0,
            "ExitCode":7,"OOMKilled":true,"StartedAt":"invalid","FinishedAt":"2025-01-02T04:04:05+01:00"},
            "RestartCount":2,"HostConfig":{"RestartPolicy":{"Name":"future-policy"}}}).to_string())),
    ]),Duration::ZERO);
    let metadata = DockerClient::new(&server.socket, vec![])
        .unwrap()
        .refresh()
        .await
        .unwrap();
    assert_eq!(metadata[&a].state, Some(ContainerState::Exited));
    assert_eq!(metadata[&a].details.exit_code, Some(7));
    assert_eq!(metadata[&a].details.oom_killed, Some(true));
    assert_eq!(metadata[&a].details.started, None);
    assert_eq!(metadata[&a].details.finished, Some(1_735_787_045.0));
    assert_eq!(
        metadata[&a].details.restart_policy.as_deref(),
        Some("unknown")
    );
    assert_eq!(metadata[&b].inspect_up, Some(false));
    assert_eq!(metadata[&b].details.restarts, None);
    assert_eq!(metadata[&b].health, Some(HealthStatus::Healthy));
}

#[tokio::test]
async fn docker_info_and_collection_options_have_separate_failure_domains() {
    let temp = tempfile::tempdir().unwrap();
    let info = json!({"ContainersRunning":2,"ContainersPaused":1,"ContainersStopped":3,"Images":4,
        "NFd":42,"NGoroutines":12,"NEventsListener":2,"NCPU":8,"MemTotal":1048576,
        "ServerVersion":"29.7.2","Driver":"overlayfs","OSType":"linux","Architecture":"x86_64",
        "CgroupVersion":"2","CgroupDriver":"systemd","DockerRootDir":"private-path"});
    let server = Server::new(
        temp.path(),
        BTreeMap::from([
            ("/containers/json?all=false".into(), (200, "[]".into())),
            ("/info".into(), (200, info.to_string())),
        ]),
        Duration::ZERO,
    );
    let client = DockerClient::with_options(
        &server.socket,
        vec![],
        DockerOptions {
            inspect: false,
            running_only: true,
            ..DockerOptions::default()
        },
    )
    .unwrap();
    let engine = client.engine().await.unwrap();
    assert_eq!(engine.containers["stopped"], 3);
    assert_eq!(engine.files, Some(42));
    assert_eq!(engine.cpu_cores, Some(8));
    assert_eq!(engine.version, "29.7.2");
    assert!(
        !serde_json::to_string(&engine)
            .unwrap()
            .contains("private-path")
    );
    assert!(client.refresh().await.unwrap().is_empty());
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn docker_inventory_bound_is_checked_before_scheduling_inspects() {
    let temp = tempfile::tempdir().unwrap();
    let list = json!([{"Id":format!("{:064x}",1)},{"Id":format!("{:064x}",2)}]);
    let server = Server::new(
        temp.path(),
        BTreeMap::from([("/containers/json?all=true".into(), (200, list.to_string()))]),
        Duration::ZERO,
    );
    let client = DockerClient::with_options(
        &server.socket,
        vec![],
        DockerOptions {
            max_containers: 1,
            ..DockerOptions::default()
        },
    )
    .unwrap();
    assert!(
        client
            .refresh()
            .await
            .unwrap_err()
            .to_string()
            .contains("bound")
    );
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn slow_inspects_obey_the_overall_refresh_deadline() {
    let temp = tempfile::tempdir().unwrap();
    let ids: Vec<_> = (0..96).map(|i| format!("{i:064x}")).collect();
    let list: Vec<_> = ids.iter().map(|id| json!({"Id":id})).collect();
    let mut routes = BTreeMap::from([(
        "/containers/json?all=true".into(),
        (200, serde_json::to_string(&list).unwrap()),
    )]);
    for id in ids {
        routes.insert(
            format!("/containers/{id}/json"),
            (
                200,
                "{\"State\":{\"Status\":\"running\",\"Running\":true}}".into(),
            ),
        );
    }
    let server = Server::new(temp.path(), routes, Duration::from_millis(500));
    let client = DockerClient::new(&server.socket, vec![]).unwrap();
    let now = Instant::now();
    let metadata = client.refresh().await.unwrap();
    assert!(now.elapsed() < Duration::from_millis(6500));
    assert!(metadata.values().any(|m| m.inspect_up == Some(false)));
    assert!(server.peak() <= 8);
}
