#!/usr/bin/env bash
# Exercise the actual Linux hierarchy and HTTP service without a shell in the image.
set -euo pipefail
image="${1:-cadvisor-rs:local}"
name="cadvisor-rs-smoke-$$"
cleanup() { docker rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT
docker run -d --name "$name" --cgroupns=host --pid=host --read-only \
  --cap-drop=ALL --security-opt=no-new-privileges \
  -v /sys/fs/cgroup:/host/cgroup:ro -v /proc:/host/proc:ro \
  -p 127.0.0.1::8080 "$image" \
  --cgroup-root=/host/cgroup --proc-root=/host/proc --disable-docker --interval-seconds=1
port="$(docker port "$name" 8080/tcp | head -1 | awk -F: '{print $NF}')"
url="http://127.0.0.1:$port"
for attempt in $(seq 1 30); do
  if curl -fsS "$url/readyz" >/dev/null; then break; fi
  if [[ "$attempt" == 30 ]]; then docker logs "$name"; exit 1; fi
  sleep 1
done
curl -fsS "$url/metrics" | python3 -c '
import sys
text = sys.stdin.read()
assert "cadvisor_rs_collection_success 1\n" in text
assert "container_cpu_usage_seconds_total{" in text
assert "machine_memory_bytes " in text
print("Linux metrics verified")
'
curl -fsS "$url/api/v1/containers" | python3 -c '
import json, sys
assert json.load(sys.stdin)["containers"]
'
docker stop --time 5 "$name" >/dev/null
test "$(docker inspect --format '{{.State.ExitCode}}' "$name")" = 0
