#!/usr/bin/env bash
# Verify real Docker HEALTHCHECK transitions through the cached Prometheus endpoint.
set -euo pipefail
image="${1:-cadvisor-rs:local}"
exporter="cadvisor-rs-health-exporter-$$"
workload="cadvisor-rs-health-workload-$$"
cleanup() { docker rm -f "$exporter" "$workload" >/dev/null 2>&1 || true; }
trap cleanup EXIT
docker run -d --name "$workload" --health-cmd='test -e /tmp/pass' \
  --health-interval=1s --health-timeout=1s --health-retries=1 \
  busybox:1.37 sh -c 'touch /tmp/pass; exec sleep 300' >/dev/null
docker run -d --name "$exporter" --cgroupns=host --pid=host --read-only \
  --cap-drop=ALL --security-opt=no-new-privileges \
  -v /sys/fs/cgroup:/host/cgroup:ro -v /proc:/host/proc:ro \
  -v /var/run/docker.sock:/var/run/docker.sock:ro -p 127.0.0.1::8080 "$image" \
  --cgroup-root=/host/cgroup --proc-root=/host/proc --containers-only \
  --interval-seconds=1 --discovery-interval-seconds=1 >/dev/null
port="$(docker port "$exporter" 8080/tcp | head -1 | awk -F: '{print $NF}')"
url="http://127.0.0.1:$port"
wait_for_health() {
  local name="$1" status="$2"
  for attempt in $(seq 1 30); do
    if curl -fsS "$url/metrics" | python3 -c '
import sys
name, status = sys.argv[1:]
rows = [line for line in sys.stdin.read().splitlines()
        if line.startswith("container_health_status{") and f",name=\"{name}\"," in line]
expected = f",status=\"{status}\"}} 1"
sys.exit(0 if len(rows) == 1 and rows[0].endswith(expected) else 1)
' "$name" "$status"; then return; fi
    sleep 1
  done
  docker logs "$exporter"
  docker inspect --format '{{json .State.Health}}' "$workload"
  return 1
}
wait_for_health "$workload" healthy
wait_for_health "$exporter" none
docker exec "$workload" rm /tmp/pass
wait_for_health "$workload" unhealthy
# An unhealthy workload does not imply that the exporter itself is unready.
curl -fsS "$url/readyz" >/dev/null
docker exec "$workload" touch /tmp/pass
wait_for_health "$workload" healthy
echo "Docker health transitions verified: healthy -> unhealthy -> healthy; no-check remains none."
