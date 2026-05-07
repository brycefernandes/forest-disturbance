#!/usr/bin/env bash
set -euo pipefail

SIMSAT_REPO_URL="${SIMSAT_REPO_URL:-https://github.com/DPhi-Space/SimSat.git}"
SIMSAT_DIR="${SIMSAT_DIR:-/content/simsat}"
SIMSAT_URL="${SIMSAT_URL:-http://127.0.0.1:9005}"
SIMSAT_DASHBOARD_URL="${SIMSAT_DASHBOARD_URL:-http://127.0.0.1:8000}"
LOG_DIR="${LOG_DIR:-/content/forest_guardian_logs}"
SMOKE_OUT_DIR="${SMOKE_OUT_DIR:-/content/simsat_smoke_outputs}"

mkdir -p "${LOG_DIR}" "${SMOKE_OUT_DIR}"
log() { printf '\n[%s] %s\n' "$(date -u +%H:%M:%S)" "$*"; }
ensure_docker() {
  if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    log "Docker is already running."
    return 0
  fi
  if ! command -v docker >/dev/null 2>&1; then
    if ! command -v apt-get >/dev/null 2>&1; then
      echo "Docker is required, docker is missing, and apt-get is unavailable." >&2
      exit 1
    fi
    log "Installing Docker packages for Colab/runtime."
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -y
    apt-get install -y docker.io docker-compose-plugin
  fi
  if docker info >/dev/null 2>&1; then
    log "Docker daemon is running."
    return 0
  fi
  log "Starting Docker daemon in the background."
  mkdir -p "${LOG_DIR}" /tmp/docker-data
  nohup dockerd --host=unix:///var/run/docker.sock --data-root=/tmp/docker-data > "${LOG_DIR}/dockerd.log" 2>&1 &
  echo $! > "${LOG_DIR}/dockerd.pid"
  for attempt in $(seq 1 90); do
    if docker info >/dev/null 2>&1; then
      log "Docker daemon is ready."
      return 0
    fi
    sleep 2
  done
  echo "Docker daemon did not become ready. Last dockerd logs:" >&2
  tail -100 "${LOG_DIR}/dockerd.log" >&2 || true
  exit 1
}

wait_http() {
  local url="$1" label="$2" attempts="${3:-120}"
  for attempt in $(seq 1 "${attempts}"); do
    if curl --fail --silent --max-time 5 "${url}" >/dev/null 2>&1; then
      log "${label} ready: ${url}"
      return 0
    fi
    sleep 2
  done
  echo "Timed out waiting for ${label}: ${url}" >&2
  return 1
}

log "Cloning official DPhi-Space/SimSat if needed"
if [[ ! -d "${SIMSAT_DIR}/.git" ]]; then
  git clone --depth 1 "${SIMSAT_REPO_URL}" "${SIMSAT_DIR}"
fi

ensure_docker

log "Starting official DPhi-Space/SimSat with docker compose"
(cd "${SIMSAT_DIR}" && docker compose up -d --build) | tee "${LOG_DIR}/simsat-smoke-docker-compose.log"
wait_http "${SIMSAT_DASHBOARD_URL}/api/telemetry/recent/" "SimSat dashboard" 180 || true
curl --fail --silent --show-error --max-time 10 "${SIMSAT_DASHBOARD_URL}/api/commands/" \
  -H 'Content-Type: application/json' \
  -d '{"command":"start","start_time":"2026-01-01T16:00:00Z","step_size_seconds":10,"replay_speed":10.0}' \
  > "${SMOKE_OUT_DIR}/dashboard_start_response.json" || true
wait_http "${SIMSAT_URL}/data/current/position" "SimSat API" 180

log "Fetching current satellite position"
curl --fail --silent --show-error "${SIMSAT_URL}/data/current/position" | tee "${SMOKE_OUT_DIR}/current_position.json"

log "Fetching DPhi README example Sentinel RGB PNG"
curl --fail --silent --show-error \
  "${SIMSAT_URL}/data/image/sentinel?lon=6.6323&lat=46.5197&timestamp=2026-03-01T16:00:00Z&spectral_bands=red,green,blue&size_km=5.0&return_type=png&window_seconds=864000" \
  -o "${SMOKE_OUT_DIR}/sentinel_rgb.png"

test -s "${SMOKE_OUT_DIR}/sentinel_rgb.png"
log "SUCCESS: SimSat smoke test completed. Outputs in ${SMOKE_OUT_DIR}"
ls -lh "${SMOKE_OUT_DIR}"
