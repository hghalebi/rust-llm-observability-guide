#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_DIR="${OTEL_SMOKE_ROOT:-${ROOT_DIR}}"

COLLECTOR_NAME="otel-smoke-check"
COLLECTOR_IMAGE="${OTEL_SMOKE_IMAGE:-otel/opentelemetry-collector-contrib:0.146.1}"
GRPC_PORT="${OTEL_SMOKE_GRPC_PORT:-14317}"
HTTP_PORT="${OTEL_SMOKE_HTTP_PORT:-14318}"
MARKER="${OTEL_SMOKE_MARKER:-learn-smoke-$(date +%s)}"
SERVICE_NAME="${OTEL_SMOKE_SERVICE:-otel-smoke-smoke}"

TMP_DIR="$(mktemp -d)"
cleanup() {
  docker rm -f "${COLLECTOR_NAME}" >/dev/null 2>&1 || true
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

cat >"${TMP_DIR}/otel-smoke-config.yaml" <<'EOF'
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317
      http:
        endpoint: 0.0.0.0:4318

exporters:
  debug:
    verbosity: detailed

processors:
  batch:

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch]
      exporters: [debug]
EOF

if command -v docker >/dev/null 2>&1; then
  :
else
  echo "docker is required for this script."
  exit 1
fi

if command -v nc >/dev/null 2>&1; then
  if nc -z 127.0.0.1 "$GRPC_PORT"; then
    echo "Port $GRPC_PORT is already in use. Set OTEL_SMOKE_GRPC_PORT to a free port."
    exit 1
  fi
else
  echo "nc is required for readiness check."
  exit 1
fi

docker rm -f "$COLLECTOR_NAME" >/dev/null 2>&1 || true
docker pull "$COLLECTOR_IMAGE" >/dev/null

docker run -d \
  --name "$COLLECTOR_NAME" \
  -p "${GRPC_PORT}:4317" \
  -p "${HTTP_PORT}:4318" \
  -v "${TMP_DIR}/otel-smoke-config.yaml:/etc/otelcol-contrib/config.yaml:ro" \
  "$COLLECTOR_IMAGE" --config=/etc/otelcol-contrib/config.yaml >/dev/null

for _ in {1..30}; do
  if nc -z 127.0.0.1 "$GRPC_PORT"; then
    break
  fi
  sleep 1
done

if ! nc -z 127.0.0.1 "$GRPC_PORT"; then
  echo "Collector did not start listening on $GRPC_PORT."
  docker logs "$COLLECTOR_NAME" | tail -n 60
  exit 1
fi

cd "$PROJECT_DIR"
RUN_LOG="$(mktemp)"
export OTEL_EXPORTER_OTLP_ENDPOINT="http://127.0.0.1:${GRPC_PORT}"
export OTEL_RESOURCE_ATTRIBUTES="service.name=${SERVICE_NAME}"
export OTEL_SMOKE_MARKER="$MARKER"

cargo run --example otel_smoke > "$RUN_LOG" 2>&1 || {
  echo "Example failed. Last lines:"
  tail -n 60 "$RUN_LOG"
  exit 1
}

COLLECTOR_LOG="${TMP_DIR}/collector.log"
found_span="false"
found_marker="false"

for attempt in {1..30}; do
  {
    docker logs "$COLLECTOR_NAME" 2>&1 || true
  } > "$COLLECTOR_LOG"

  if grep -Fq "otel_smoke_probe" "$COLLECTOR_LOG"; then
    found_span="true"
  else
    sleep 1
    continue
  fi

  if grep -Fq "$MARKER" "$COLLECTOR_LOG"; then
    found_marker="true"
  fi

  if [[ "${found_span}" == "true" ]]; then
    break
  fi
done

echo
echo "--- collector output (tail) ---"
tail -n 140 "$COLLECTOR_LOG"
echo

if [[ -f "$COLLECTOR_LOG" ]]; then
  if grep -Fq "$MARKER" "$COLLECTOR_LOG"; then
    found_marker="true"
  fi
fi

if [[ "${found_span}" == "true" ]]; then
  echo "otel_smoke_probe: found in collector output"
  echo "collector_receives: true"
  echo "marker_match: ${found_marker}"
else
  echo "otel_smoke_probe: not found in collector output"
  echo "collector_receives: false"
  echo "marker_match: false"
  echo "If you still need full collector details, share the full contents at:"
  echo "  tail -n 300 \"$COLLECTOR_LOG\""
  exit 1
fi

echo "--- run output ---"
sed -n '1,120p' "$RUN_LOG"
