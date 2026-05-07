#!/usr/bin/env bash
set -euo pipefail

SIMSAT_URL="${SIMSAT_URL:-http://localhost:9005}"
VLM_URL="${VLM_URL:-http://localhost:8080}"
VLM_MODEL="${VLM_MODEL:-lfm2.5-vl}"
LON="${LON:--60.025}"
LAT="${LAT:--3.125}"
BASELINE_TIMESTAMP="${BASELINE_TIMESTAMP:-2026-01-01T00:00:00Z}"
CURRENT_TIMESTAMP="${CURRENT_TIMESTAMP:-2026-02-01T00:00:00Z}"
OUT_DIR="${OUT_DIR:-data/alert_packets/e2e-latest}"

printf 'Forest Guardian real E2E check\n'
printf '  SimSat: %s\n' "${SIMSAT_URL}"
printf '  LFM VLM: %s model=%s\n' "${VLM_URL}" "${VLM_MODEL}"
printf '  AOI point: lon=%s lat=%s\n' "${LON}" "${LAT}"

if ! curl --fail --silent --show-error --max-time 5 "${SIMSAT_URL}/data/current/position" >/dev/null; then
  echo "SimSat health probe failed. Start SimSat before running real E2E." >&2
  exit 1
fi

if ! curl --fail --silent --show-error --max-time 5 "${VLM_URL}/v1/models" >/dev/null; then
  echo "LFM VLM health probe failed. Start llama-server/llama.cpp before running real E2E." >&2
  exit 1
fi

rm -rf "${OUT_DIR}"

cargo run -- run \
  --simsat-url "${SIMSAT_URL}" \
  --vlm-url "${VLM_URL}" \
  --vlm-model "${VLM_MODEL}" \
  --lon "${LON}" \
  --lat "${LAT}" \
  --baseline-timestamp "${BASELINE_TIMESTAMP}" \
  --current-timestamp "${CURRENT_TIMESTAMP}" \
  --out-dir "${OUT_DIR}"

cargo run -- verify --packet-dir "${OUT_DIR}"

test -s "${OUT_DIR}/manifest.json"
test -s "${OUT_DIR}/alert.json"
test -s "${OUT_DIR}/vlm_reports/role_01_triage.json"
test -s "${OUT_DIR}/vlm_reports/role_03_narrative.json"

echo "Real E2E completed and evidence packet verified: ${OUT_DIR}"
