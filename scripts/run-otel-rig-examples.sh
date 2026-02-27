#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_DIR="${OTEL_EXAMPLES_ROOT:-${ROOT_DIR}}"

echo "Running Rig/Otel tutorial examples from: $PROJECT_DIR"
echo

cd "$PROJECT_DIR"

examples_to_run=(
  otel_smoke
  gemini_rig_basic
  gemini_rig_tools
  gemini_multi_agent
)

if [[ -z "${GEMINI_API_KEY:-}" ]]; then
  echo "WARN: GEMINI_API_KEY is not set."
  echo "   Gemini examples are listed as SKIP (expected telemetry path still runs: otel_smoke)."
  echo
fi

passed=0
failed=0
skipped=0

for example in "${examples_to_run[@]}"; do
  run_log="$(mktemp)"

  if [[ "$example" == "otel_smoke" && -z "${OTEL_EXPORTER_OTLP_ENDPOINT:-}" ]]; then
    echo "[SKIP] $example -> set OTEL_EXPORTER_OTLP_ENDPOINT to run telemetry smoke"
    skipped=$((skipped + 1))
    continue
  fi

  if [[ "$example" != "otel_smoke" && -z "${GEMINI_API_KEY:-}" ]]; then
    echo "[SKIP] $example -> missing GEMINI_API_KEY"
    skipped=$((skipped + 1))
    continue
  fi

  echo "▶ cargo run --example $example"
  if cargo run --example "$example" > "$run_log" 2>&1; then
    passed=$((passed + 1))
    echo "   status: PASS"
    tail -n 40 "$run_log"
  else
    failed=$((failed + 1))
    echo "   status: FAIL"
    echo "   last lines:"
    tail -n 80 "$run_log"
  fi
  echo
done

echo "Summary: PASS=$passed  FAIL=$failed  SKIP=$skipped"
if [[ $failed -gt 0 ]]; then
  echo "One or more examples failed."
  exit 1
fi

echo "All runnable examples completed."
