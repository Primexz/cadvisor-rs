#!/usr/bin/env bash
# Verify container metrics with real limits, a volume, a stopped container and Docker inspect.
set -euo pipefail
image="${1:-cadvisor-rs:local}"
prefix="cadvisor-rs-metrics-$$"
exporter="$prefix-exporter"
running="$prefix-running"
stopped="$prefix-stopped"
volume="$prefix-volume"
cleanup() {
  docker rm -f "$exporter" "$running" "$stopped" >/dev/null 2>&1 || true
  docker volume rm "$volume" >/dev/null 2>&1 || true
}
trap cleanup EXIT
docker volume create "$volume" >/dev/null
docker run -d --name "$running" --cpus=0.5 --memory=64m --memory-reservation=32m \
  --memory-swap=128m --pids-limit=64 -v "$volume:/data" busybox:1.37 \
  sh -c 'dd if=/dev/zero of=/writable bs=1M count=1; exec sleep 300' >/dev/null
docker create --name "$stopped" busybox:1.37 sh -c 'exit 7' >/dev/null
docker start -a "$stopped" >/dev/null || true
docker run -d --name "$exporter" --cgroupns=host --pid=host --read-only \
  --cap-drop=ALL --cap-add=SYS_PTRACE --security-opt=no-new-privileges \
  -v /sys/fs/cgroup:/host/cgroup:ro -v /proc:/host/proc:ro \
  -v /var/run/docker.sock:/var/run/docker.sock:ro -p 127.0.0.1::8080 "$image" \
  --cgroup-root=/host/cgroup --proc-root=/host/proc --containers-only \
  --docker-size-metrics --interval-seconds=1 --discovery-interval-seconds=1 >/dev/null
port="$(docker port "$exporter" 8080/tcp | head -1 | awk -F: '{print $NF}')"
url="http://127.0.0.1:$port"
for attempt in $(seq 1 30); do
  if curl -fsS "$url/readyz" >/dev/null; then break; fi
  if [[ "$attempt" == 30 ]]; then docker logs "$exporter"; exit 1; fi
  sleep 1
done
curl -fsS "$url/api/v1/containers" | python3 -c '
import json,sys
sample=json.load(sys.stdin)
by_name={c["metadata"]["name"]:c for c in sample["containers"]}
running,stopped=by_name[sys.argv[1]],by_name[sys.argv[2]]
assert running["cpu"]["quota"]==50000
assert running["cpu"]["period"]==100000
assert running["memory"]["limit"]==64*1024*1024
assert running["pids_limit"]==64
assert running["processes"]["count"]>=1
assert running["processes"]["files"]>=3, running["processes"]
assert running["processes"]["errors"]==0,running["processes"]
assert running["metadata"]["inspect_up"]
assert running["metadata"]["details"]["started"]>0
assert running["metadata"]["details"]["restarts"]==0
assert running["metadata"]["writable_bytes"]>=1024*1024
assert any(f["device"]=="/data" and f["limit"]>0 for f in running["filesystems"])
assert any(f["device"]=="rootfs" and f["usage"]>=1024*1024 for f in running["filesystems"])
assert stopped["metadata"]["state"]=="exited"
assert stopped["metadata"]["details"]["running"] is False
assert stopped["metadata"]["details"]["exit_code"]==7
assert stopped["cpu"]["usage"] is None and stopped["memory"]["usage"] is None
assert sample["docker_engine"]["containers"]["stopped"]>=1
print("Limits, process/FDs, volume capacity, writable size and stopped inventory verified")
' "$running" "$stopped"
curl -fsS "$url/metrics" | python3 -c '
import sys
text=sys.stdin.read()
required=["container_restarts_total", "container_start_time_seconds", "container_running",
          "container_last_exit_code", "container_memory_max_usage_bytes", "container_memory_failures_total",
          "container_processes", "container_file_descriptors", "container_tasks_state",
          "container_network_tcp_usage_total", "container_fs_limit_bytes", "container_fs_usage_bytes",
          "container_spec_cpu_shares", "docker_engine_info"]
for name in required:
    assert any(line.startswith(name+"{") for line in text.splitlines()),name
types=[line for line in text.splitlines() if line.startswith("# TYPE ")]
print(f"Container metrics verified ({len(types)} metric families)")
'
