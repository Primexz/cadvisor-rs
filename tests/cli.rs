//! Execute the actual binary against fixtures and check CLI failures.

mod common;

use std::process::Command;

struct Daemon(std::process::Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn get(port: u16, path: &str) -> String {
    use std::io::{Read, Write};
    let Ok(mut socket) = std::net::TcpStream::connect(("127.0.0.1", port)) else {
        return String::new();
    };
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .unwrap();
    write!(
        socket,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).unwrap();
    response
}

fn wait_for(mut predicate: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !predicate() {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(6),
            "daemon condition timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[cfg(unix)]
#[test]
fn daemon_refreshes_recovers_and_gracefully_handles_sigterm() {
    let temp = common::fixture();
    common::v2(temp.path(), "", 3_000_000);
    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reserved.local_addr().unwrap().port();
    drop(reserved);
    let child = Command::new(env!("CARGO_BIN_EXE_cadvisor-rs"))
        .args([
            "--interval-seconds=1",
            "--discovery-interval-seconds=1",
            "--listen",
        ])
        .arg(format!("127.0.0.1:{port}"))
        .arg("--docker-socket")
        .arg(temp.path().join("missing.sock"))
        .arg("--cgroup-root")
        .arg(temp.path().join("cgroup"))
        .arg("--proc-root")
        .arg(temp.path().join("proc"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut child = Daemon(child);
    wait_for(|| get(port, "/readyz").starts_with("HTTP/1.1 200"));
    assert!(get(port, "/metrics").contains("cpu=\"total\"} 3\n"));
    common::put(temp.path(), "cgroup/cpu.stat", "usage_usec 4000000\n");
    wait_for(|| get(port, "/metrics").contains("cpu=\"total\"} 4\n"));
    std::fs::remove_dir_all(temp.path().join("cgroup")).unwrap();
    wait_for(|| get(port, "/readyz").starts_with("HTTP/1.1 503"));
    assert!(get(port, "/metrics").contains("cadvisor_rs_collection_success 0\n"));
    common::v2(temp.path(), "", 5_000_000);
    wait_for(|| get(port, "/readyz").starts_with("HTTP/1.1 200"));
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.0.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    wait_for(|| child.0.try_wait().unwrap().is_some());
    assert!(child.0.wait().unwrap().success());
}

#[test]
fn once_prints_metrics_and_rejects_invalid_configuration() {
    let temp = common::fixture();
    common::v2(temp.path(), "", 3_000_000);
    let output = Command::new(env!("CARGO_BIN_EXE_cadvisor-rs"))
        .args(["--once", "--disable-docker", "--cgroup-root"])
        .arg(temp.path().join("cgroup"))
        .arg("--proc-root")
        .arg(temp.path().join("proc"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("cadvisor_rs_collection_success 1\n"));
    assert!(stdout.contains("cpu=\"total\"} 3\n"));
    let output = Command::new(env!("CARGO_BIN_EXE_cadvisor-rs"))
        .args(["--interval-seconds=0"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let output = Command::new(env!("CARGO_BIN_EXE_cadvisor-rs"))
        .args(["--once", "--disable-docker", "--cgroup-root"])
        .arg(temp.path().join("nonexistent"))
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[cfg(unix)]
#[test]
fn docker_metadata_and_engine_are_wired_into_once_and_daemon_snapshots() {
    let temp = common::fixture();
    common::v2(temp.path(), &format!("docker/{}", common::ID), 3_000_000);
    let list = serde_json::json!([{"Id":common::ID,"Names":["/web"],"State":"running"}]);
    let inspect = serde_json::json!({"State":{"Status":"running","Running":true,"Pid":123,"StartedAt":"2025-01-02T03:04:05Z"},"RestartCount":3});
    let info = serde_json::json!({"ServerVersion":"29.7.2","ContainersRunning":1,"Images":4});
    let server = common::http::Server::new(
        temp.path(),
        std::collections::BTreeMap::from([
            ("/containers/json?all=true".into(), (200, list.to_string())),
            (
                format!("/containers/{}/json", common::ID),
                (200, inspect.to_string()),
            ),
            ("/info".into(), (200, info.to_string())),
        ]),
        std::time::Duration::ZERO,
    );
    let configure = |command: &mut Command| {
        command
            .arg("--docker-socket")
            .arg(&server.socket)
            .arg("--cgroup-root")
            .arg(temp.path().join("cgroup"))
            .arg("--proc-root")
            .arg(temp.path().join("proc"));
    };
    let mut command = Command::new(env!("CARGO_BIN_EXE_cadvisor-rs"));
    configure(&mut command);
    command.arg("--once");
    let output = command.output().unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("docker_engine_images 4\n"));
    assert!(
        text.lines()
            .any(|line| line.starts_with("container_restarts_total{") && line.ends_with("} 3"))
    );
    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reserved.local_addr().unwrap().port();
    drop(reserved);
    let mut command = Command::new(env!("CARGO_BIN_EXE_cadvisor-rs"));
    configure(&mut command);
    command
        .args([
            "--interval-seconds=1",
            "--discovery-interval-seconds=1",
            "--listen",
        ])
        .arg(format!("127.0.0.1:{port}"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = Daemon(command.spawn().unwrap());
    wait_for(|| get(port, "/readyz").starts_with("HTTP/1.1 200"));
    assert!(get(port, "/metrics").contains("docker_engine_images 4\n"));
    assert!(get(port, "/metrics").contains("cadvisor_rs_docker_up 1\n"));
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.0.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    wait_for(|| child.0.try_wait().unwrap().is_some());
    assert!(child.0.wait().unwrap().success());
}
