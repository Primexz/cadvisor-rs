//! Unix HTTP metadata fixtures exercise the actual client, not a mock trait.
#![cfg(unix)]

mod common;

use std::{
    io::{Read, Write},
    os::unix::net::UnixListener,
    thread,
};

use cadvisor_rs::{
    docker::DockerClient,
    model::{ContainerState, HealthStatus},
};

fn serve(socket: &std::path::Path, status: &str, body: String) -> thread::JoinHandle<String> {
    let listener = UnixListener::bind(socket).unwrap();
    let status = status.to_owned();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 4096];
        let len = stream.read(&mut request).unwrap();
        let header = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(header.as_bytes()).unwrap();
        let _ = stream.write_all(body.as_bytes());
        String::from_utf8(request[..len].to_vec()).unwrap()
    })
}

#[tokio::test]
async fn docker_enrichment_reads_names_images_and_only_allowlisted_labels() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("docker.sock");
    let server = serve(
        &socket,
        "200 OK",
        format!(
            r#"[{{"Id":"{}","Names":["/web"],"Image":"demo:1","Created":123,"Labels":{{"app":"web","secret":"hidden"}}}},{{"Id":"short","Names":[]}}]"#,
            common::ID
        ),
    );
    let client = DockerClient::new(&socket, vec!["app".to_owned()]).unwrap();
    let metadata = client.refresh().await.unwrap();
    assert_eq!(metadata.len(), 1);
    let item = &metadata[common::ID];
    assert_eq!(item.name, "web");
    assert_eq!(item.image, "demo:1");
    assert_eq!(item.created, Some(123));
    assert_eq!(item.labels.len(), 1);
    assert_eq!(item.labels["app"], "web");
    assert!(
        server
            .join()
            .unwrap()
            .starts_with("GET /containers/json?all=true HTTP/1.1")
    );
}

#[tokio::test]
async fn docker_client_rejects_transport_http_and_json_failures() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("docker.sock");
    let client = DockerClient::new(&socket, Vec::new()).unwrap();
    assert!(client.refresh().await.is_err());
    let server = serve(&socket, "500 Internal Server Error", "failure".into());
    assert!(client.refresh().await.is_err());
    server.join().unwrap();
    std::fs::remove_file(&socket).unwrap();
    let server = serve(&socket, "200 OK", "{invalid".into());
    assert!(client.refresh().await.is_err());
    server.join().unwrap();
}

#[tokio::test]
async fn docker_client_bounds_response_memory() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("docker.sock");
    let server = serve(&socket, "200 OK", " ".repeat(8 * 1024 * 1024 + 1));
    let client = DockerClient::new(&socket, Vec::new()).unwrap();
    let error = client.refresh().await.unwrap_err();
    assert!(error.to_string().contains("exceeds 8 MiB"));
    server.join().unwrap();
}

#[tokio::test]
async fn docker_structured_health_is_authoritative_and_states_are_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("docker.sock");
    let cases = [
        (
            "healthy",
            HealthStatus::Healthy,
            "running",
            ContainerState::Running,
            Some(0),
        ),
        (
            "unhealthy",
            HealthStatus::Unhealthy,
            "paused",
            ContainerState::Paused,
            Some(3),
        ),
        (
            "starting",
            HealthStatus::Starting,
            "restarting",
            ContainerState::Restarting,
            Some(2),
        ),
        (
            "none",
            HealthStatus::NoHealthcheck,
            "created",
            ContainerState::Created,
            None,
        ),
        (
            "future-health",
            HealthStatus::Unknown,
            "future-state",
            ContainerState::Unknown,
            None,
        ),
        (
            "none",
            HealthStatus::NoHealthcheck,
            "exited",
            ContainerState::Exited,
            None,
        ),
        (
            "none",
            HealthStatus::NoHealthcheck,
            "removing",
            ContainerState::Removing,
            None,
        ),
        (
            "none",
            HealthStatus::NoHealthcheck,
            "dead",
            ContainerState::Dead,
            None,
        ),
    ];
    let response: Vec<_> = cases
        .iter()
        .enumerate()
        .map(|(i, case)| {
            serde_json::json!({
                "Id": format!("{i:064x}"), "Names": [format!("/test-{i}")],
                "State": case.2,
                // Contradictory human status must not override structured data.
                "Status": "Up 5 seconds (unhealthy)",
                "Health": {"Status": case.0, "FailingStreak": case.4.unwrap_or(99)}
            })
        })
        .collect();
    let server = serve(&socket, "200 OK", serde_json::to_string(&response).unwrap());
    let metadata = DockerClient::new(&socket, Vec::new())
        .unwrap()
        .refresh()
        .await
        .unwrap();
    for (i, case) in cases.iter().enumerate() {
        let item = &metadata[&format!("{i:064x}")];
        assert_eq!(item.health, Some(case.1));
        assert_eq!(item.state, Some(case.3));
        assert_eq!(item.health_failing_streak, case.4);
        assert_eq!(serde_json::to_value(case.1).unwrap(), case.1.as_str());
        assert_eq!(serde_json::to_value(case.3).unwrap(), case.3.as_str());
    }
    server.join().unwrap();
}

#[tokio::test]
async fn docker_legacy_health_handles_no_check_startup_and_unknown_statuses() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("docker.sock");
    let cases = [
        ("Up 5 seconds (healthy)", HealthStatus::Healthy),
        ("Up 5 seconds (unhealthy)", HealthStatus::Unhealthy),
        ("Up 5 seconds (health: starting)", HealthStatus::Starting),
        ("Up 5 seconds", HealthStatus::NoHealthcheck),
        ("Up 5 seconds (Paused)", HealthStatus::Unknown),
        ("", HealthStatus::Unknown),
        ("Up 5 seconds (future-health)", HealthStatus::Unknown),
    ];
    let response: Vec<_> = cases.iter().enumerate().map(|(i, case)| {
        serde_json::json!({"Id": format!("{i:064x}"), "Status": case.0, "Health": null})
    }).collect();
    let server = serve(&socket, "200 OK", serde_json::to_string(&response).unwrap());
    let metadata = DockerClient::new(&socket, Vec::new())
        .unwrap()
        .refresh()
        .await
        .unwrap();
    for (i, case) in cases.iter().enumerate() {
        let item = &metadata[&format!("{i:064x}")];
        assert_eq!(item.health, Some(case.1));
        assert_eq!(item.health_failing_streak, None);
        assert_eq!(item.state, None);
    }
    server.join().unwrap();
}
