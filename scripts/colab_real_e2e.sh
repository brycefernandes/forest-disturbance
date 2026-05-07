#!/usr/bin/env bash
set -euo pipefail

# Colab-ready real E2E bootstrap for Forest Guardian.
# It installs Rust if needed, builds llama.cpp, starts the smallest LFM2.5-VL
# GGUF target through llama-server, ensures a real SimSat endpoint is available,
# runs the production CLI, and verifies the evidence packet.
#
# Required if SimSat is not already running:
#   SIMSAT_REPO_URL=<git url containing a runnable SimSat service>
# Optional:
#   SIMSAT_START_CMD='command to start SimSat from inside the cloned repo'
#   FOREST_GUARDIAN_REPO=<git url for this repo, if running outside a clone>

ROOT_DIR="${ROOT_DIR:-/content/forest-disturbance}"
FOREST_GUARDIAN_REPO="${FOREST_GUARDIAN_REPO:-}"
LLAMA_CPP_DIR="${LLAMA_CPP_DIR:-/content/llama.cpp}"
LLAMA_CPP_REPO="${LLAMA_CPP_REPO:-https://github.com/ggml-org/llama.cpp.git}"
LFM25_REPO="${LFM25_REPO:-LiquidAI/LFM2.5-VL-450M-GGUF}"
LFM25_QUANT="${LFM25_QUANT:-Q4_0}"
VLM_HOST="${VLM_HOST:-127.0.0.1}"
VLM_PORT="${VLM_PORT:-8080}"
VLM_URL="${VLM_URL:-http://${VLM_HOST}:${VLM_PORT}}"
VLM_MODEL="${VLM_MODEL:-lfm2.5-vl}"
SIMSAT_URL="${SIMSAT_URL:-http://127.0.0.1:9005}"
SIMSAT_REPO_URL="${SIMSAT_REPO_URL:-https://github.com/DPhi-Space/SimSat.git}"
SIMSAT_DIR="${SIMSAT_DIR:-/content/simsat}"
SIMSAT_START_CMD="${SIMSAT_START_CMD:-}"
SIMSAT_DASHBOARD_URL="${SIMSAT_DASHBOARD_URL:-http://127.0.0.1:8000}"
OUT_DIR="${OUT_DIR:-data/alert_packets/colab-e2e}"
LON="${LON:--60.025}"
LAT="${LAT:--3.125}"
BASELINE_TIMESTAMP="${BASELINE_TIMESTAMP:-2026-01-01T00:00:00Z}"
CURRENT_TIMESTAMP="${CURRENT_TIMESTAMP:-2026-02-01T00:00:00Z}"
LOG_DIR="${LOG_DIR:-/content/forest_guardian_logs}"

mkdir -p "${LOG_DIR}"

log() { printf '\n[%s] %s\n' "$(date -u +%H:%M:%S)" "$*"; }
run() { log "$*"; "$@"; }

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
  local url="$1"
  local label="$2"
  local max_attempts="${3:-120}"
  for attempt in $(seq 1 "${max_attempts}"); do
    if curl --fail --silent --max-time 5 "${url}" >/dev/null 2>&1; then
      log "${label} is ready: ${url}"
      return 0
    fi
    sleep 2
  done
  echo "Timed out waiting for ${label}: ${url}" >&2
  return 1
}

install_system_deps() {
  local missing=()
  for bin in git cmake curl python3; do
    if ! command -v "${bin}" >/dev/null 2>&1; then
      missing+=("${bin}")
    fi
  done
  if command -v ninja >/dev/null 2>&1 || command -v make >/dev/null 2>&1; then
    :
  else
    missing+=("ninja-build")
  fi
  if command -v g++ >/dev/null 2>&1 || command -v clang++ >/dev/null 2>&1; then
    :
  else
    missing+=("build-essential")
  fi
  if [[ "${FORCE_APT_INSTALL:-0}" != "1" && "${#missing[@]}" -eq 0 ]]; then
    log "System build dependencies already available; skipping apt-get."
    return 0
  fi
  if ! command -v apt-get >/dev/null 2>&1; then
    echo "Missing required tools (${missing[*]:-unknown}) and apt-get is unavailable." >&2
    exit 1
  fi
  export DEBIAN_FRONTEND=noninteractive
  run apt-get update -y
  run apt-get install -y git cmake ninja-build build-essential curl pkg-config ca-certificates jq python3 python3-pip
}

install_rust_if_needed() {
  if command -v cargo >/dev/null 2>&1; then
    log "Rust already available: $(cargo --version)"
    return 0
  fi
  log "Installing Rust toolchain via rustup"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  # shellcheck source=/dev/null
  source "${HOME}/.cargo/env"
  cargo --version
}

prepare_repo() {
  if [[ -f Cargo.toml && -d src ]]; then
    ROOT_DIR="$(pwd)"
    log "Using current Forest Guardian checkout: ${ROOT_DIR}"
    return 0
  fi
  if [[ -d "${ROOT_DIR}/.git" ]]; then
    log "Using existing Forest Guardian checkout: ${ROOT_DIR}"
    cd "${ROOT_DIR}"
    return 0
  fi
  if [[ -z "${FOREST_GUARDIAN_REPO}" ]]; then
    cat >&2 <<'MSG'
FOREST_GUARDIAN_REPO is required when this script is not run from an existing clone.
Example Colab command:
  FOREST_GUARDIAN_REPO=https://github.com/<you>/forest-disturbance.git \
  SIMSAT_REPO_URL=https://github.com/<you>/simsat.git \
  SIMSAT_START_CMD='python -m simsat --host 0.0.0.0 --port 9005' \
  bash scripts/colab_real_e2e.sh
MSG
    exit 1
  fi
  run git clone --depth 1 "${FOREST_GUARDIAN_REPO}" "${ROOT_DIR}"
  cd "${ROOT_DIR}"
}

build_llama_cpp() {
  if [[ ! -d "${LLAMA_CPP_DIR}/.git" ]]; then
    run git clone --depth 1 "${LLAMA_CPP_REPO}" "${LLAMA_CPP_DIR}"
  fi
  pushd "${LLAMA_CPP_DIR}" >/dev/null
  local cmake_args=(-B build -DCMAKE_BUILD_TYPE=Release)
  if command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi >/dev/null 2>&1; then
    cmake_args+=(-DGGML_CUDA=ON)
  fi
  run cmake "${cmake_args[@]}"
  run cmake --build build --config Release -j "$(nproc)"
  test -x build/bin/llama-server
  popd >/dev/null
}

start_lfm25_server() {
  if curl --fail --silent --max-time 5 "${VLM_URL}/v1/models" >/dev/null 2>&1; then
    log "LFM VLM server already running at ${VLM_URL}"
    return 0
  fi
  local server="${LLAMA_CPP_DIR}/build/bin/llama-server"
  if [[ ! -x "${server}" ]]; then
    echo "Missing llama-server at ${server}" >&2
    exit 1
  fi
  log "Starting llama-server with ${LFM25_REPO}:${LFM25_QUANT}; this downloads the model from Hugging Face on first run."
  "${server}" \
    -hf "${LFM25_REPO}:${LFM25_QUANT}" \
    --host "${VLM_HOST}" \
    --port "${VLM_PORT}" \
    -c 2048 \
    --temp 0.0 \
    --seed 42 \
    > "${LOG_DIR}/llama-server.log" 2>&1 &
  echo $! > "${LOG_DIR}/llama-server.pid"
  wait_http "${VLM_URL}/v1/models" "LFM2.5 llama-server" 180
}

ensure_simsat() {
  if curl --fail --silent --max-time 5 "${SIMSAT_URL}/data/current/position" >/dev/null 2>&1; then
    log "SimSat already running at ${SIMSAT_URL}"
    return 0
  fi
  if [[ ! -d "${SIMSAT_DIR}/.git" ]]; then
    run git clone --depth 1 "${SIMSAT_REPO_URL}" "${SIMSAT_DIR}"
  fi
  if [[ -n "${SIMSAT_START_CMD}" ]]; then
    log "Starting real SimSat with SIMSAT_START_CMD from ${SIMSAT_DIR}"
    (cd "${SIMSAT_DIR}" && bash -lc "${SIMSAT_START_CMD}") > "${LOG_DIR}/simsat.log" 2>&1 &
    echo $! > "${LOG_DIR}/simsat.pid"
  else
    ensure_docker
    log "Starting official DPhi-Space/SimSat with docker compose from ${SIMSAT_DIR}"
    (cd "${SIMSAT_DIR}" && docker compose up -d --build) | tee "${LOG_DIR}/simsat-docker-compose.log"
  fi
  wait_http "${SIMSAT_DASHBOARD_URL}/api/telemetry/recent/" "SimSat dashboard" 180 || true
  if curl --fail --silent --max-time 5 "${SIMSAT_DASHBOARD_URL}/api/commands/"       -H 'Content-Type: application/json'       -d '{"command":"start","start_time":"2026-01-01T16:00:00Z","step_size_seconds":10,"replay_speed":10.0}' >/dev/null 2>&1; then
    log "Started SimSat simulation through dashboard command API."
  else
    log "Dashboard command API did not accept start command; continuing if SimSat API is ready."
  fi
  wait_http "${SIMSAT_URL}/data/current/position" "SimSat API" 180
}

run_forest_guardian_e2e() {
  cd "${ROOT_DIR}"
  run cargo test
  rm -rf "${OUT_DIR}"
  run cargo run -- run \
    --simsat-url "${SIMSAT_URL}" \
    --vlm-url "${VLM_URL}" \
    --vlm-model "${VLM_MODEL}" \
    --lon "${LON}" \
    --lat "${LAT}" \
    --baseline-timestamp "${BASELINE_TIMESTAMP}" \
    --current-timestamp "${CURRENT_TIMESTAMP}" \
    --out-dir "${OUT_DIR}"
  run cargo run -- verify --packet-dir "${OUT_DIR}"
  test -s "${OUT_DIR}/manifest.json"
  test -s "${OUT_DIR}/alert.json"
  test -s "${OUT_DIR}/vlm_reports/role_01_triage.json"
  test -s "${OUT_DIR}/vlm_reports/role_03_narrative.json"
  log "SUCCESS: real Colab E2E completed. Evidence packet: ${ROOT_DIR}/${OUT_DIR}"
  log "Key outputs:"
  sed -n '1,80p' "${OUT_DIR}/alert.json" || true
  sed -n '1,120p' "${OUT_DIR}/manifest.json" || true
}

main() {
  log "Forest Guardian Colab real E2E bootstrap"
  install_system_deps
  install_rust_if_needed
  prepare_repo
  build_llama_cpp
  start_lfm25_server
  ensure_simsat
  run_forest_guardian_e2e
}

main "$@"
