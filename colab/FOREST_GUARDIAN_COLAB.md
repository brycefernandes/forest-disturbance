# Forest Guardian Real Colab E2E with DPhi-Space/SimSat

These are the Colab commands to first prove the official DPhi-Space/SimSat simulation is running, then run the full Forest Guardian pipeline with the smallest LFM2.5 VLM through llama.cpp.


## Why your previous paste failed

This failed because the example URL contained `<you>`:

```bash
git clone https://github.com/<you>/forest-disturbance.git forest-disturbance
```

In bash, `<you>` is interpreted as input redirection from a file named `you`, so Colab reports `bash: line 4: you: No such file or directory`. Use `YOUR_GITHUB_USER` or your real GitHub URL instead, without angle brackets.

## Verify you pulled the latest script

Before running the smoke test, check that Colab has the updated Docker-bootstrap version of the script:

```bash
%%bash
cd /content/forest-disturbance
git rev-parse --short HEAD || true
if grep -n "ensure_docker\|Installing Docker packages" scripts/colab_dphi_simsat_smoke.sh; then
  echo "OK: Docker-bootstrap smoke script is present."
else
  echo "OLD SCRIPT: Docker-bootstrap smoke script is missing. Run the refresh cell below." >&2
fi
```

If this prints `OLD SCRIPT` and you still see `Docker is required for the official DPhi-Space/SimSat quick start`, Colab is running an old commit. Pull the latest branch or copy the patched script from this PR before retrying. The check cell intentionally does not fail anymore; it prints the diagnosis and lets you continue to the refresh cell.

Fastest fix in Colab if your GitHub repo has not been updated yet:

```bash
%%bash
set -euo pipefail
cd /content/forest-disturbance
python3 - <<'PYCODE'
from pathlib import Path
url = 'https://raw.githubusercontent.com/brycefernandes/forest-disturbance/main/scripts/colab_dphi_simsat_smoke.sh'
import urllib.request
Path('scripts/colab_dphi_simsat_smoke.sh').write_bytes(urllib.request.urlopen(url).read())
PYCODE
chmod +x scripts/colab_dphi_simsat_smoke.sh
if grep -n "ensure_docker\|Installing Docker packages" scripts/colab_dphi_simsat_smoke.sh; then
  echo "OK: refreshed Docker-bootstrap smoke script."
else
  echo "Refresh failed: Docker-bootstrap code still missing." >&2
  exit 1
fi
scripts/colab_dphi_simsat_smoke.sh
```

## Self-contained smoke cell that does not depend on GitHub raw branch freshness

If your GitHub `main` branch is behind, use this cell. It writes the current Docker-bootstrap smoke script directly into the Colab checkout, then runs it.

```bash
%%bash
set -euo pipefail
cd /content/forest-disturbance
cat > scripts/colab_dphi_simsat_smoke.sh <<'SCRIPT'
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
SCRIPT
chmod +x scripts/colab_dphi_simsat_smoke.sh
grep -n "ensure_docker\|Installing Docker packages" scripts/colab_dphi_simsat_smoke.sh
scripts/colab_dphi_simsat_smoke.sh
```

## Cell 1 — SimSat simulation smoke test

This starts the official `DPhi-Space/SimSat` repository with Docker Compose, starts the satellite simulation through the dashboard API, then downloads one Sentinel RGB image.

Before running this cell, the Forest Guardian code must exist in Colab. Choose **one** of these two options.

### Option A — clone your real GitHub repo

Use this if you have pushed Forest Guardian to GitHub. Replace the URL with the actual repo URL. There is no default URL in this workspace because this Codex container has no configured Git remote.

```bash
%%bash
set -euo pipefail
cd /content

FOREST_GUARDIAN_REPO=""
if [[ -z "${FOREST_GUARDIAN_REPO}" ]]; then
  cat >&2 <<'MSG'
Set FOREST_GUARDIAN_REPO to the real GitHub URL first.
Example:
  FOREST_GUARDIAN_REPO="https://github.com/alice/forest-disturbance.git"

Do not paste placeholders such as <you> or YOUR_GITHUB_USER.
MSG
  exit 1
fi

rm -rf forest-disturbance
git clone "${FOREST_GUARDIAN_REPO}" forest-disturbance
cd forest-disturbance

scripts/colab_dphi_simsat_smoke.sh
```

### Option B — upload a zip if you do not have a GitHub URL yet

Run this Python cell, upload a zip of the repo, then continue with Cell 2 after the smoke test finishes.

```python
from google.colab import files
import pathlib, shutil, zipfile

uploaded = files.upload()
zip_name = next(name for name in uploaded if name.endswith('.zip'))
root = pathlib.Path('/content')
repo_dir = root / 'forest-disturbance'
if repo_dir.exists():
    shutil.rmtree(repo_dir)
with zipfile.ZipFile(zip_name) as zf:
    zf.extractall(root)
# If the zip contains one top-level folder, rename it to /content/forest-disturbance.
folders = [p for p in root.iterdir() if p.is_dir() and p.name.startswith('forest') and p.name != 'forest-disturbance']
if not repo_dir.exists() and folders:
    folders[0].rename(repo_dir)
assert (repo_dir / 'scripts' / 'colab_dphi_simsat_smoke.sh').exists(), 'Uploaded zip does not look like this repo'
```

Then run:

```bash
%%bash
set -euo pipefail
cd /content/forest-disturbance
scripts/colab_dphi_simsat_smoke.sh
```

Expected final line:

```text
SUCCESS: SimSat smoke test completed. Outputs in /content/simsat_smoke_outputs
```

Expected output files:

- `/content/simsat_smoke_outputs/current_position.json`
- `/content/simsat_smoke_outputs/sentinel_rgb.png`

> Note: the official SimSat quick start uses `docker compose up`. The smoke script now attempts to install `docker.io`/`docker-compose-plugin` and start `dockerd` when Docker is missing. If the Colab/runtime blocks Docker daemon startup, run SimSat on another machine and set `SIMSAT_URL` to that API URL.

## Cell 2 — Full Forest Guardian E2E

After the smoke test passes, run the real pipeline:

```bash
%%bash
set -euo pipefail
cd /content/forest-disturbance

export SIMSAT_URL="http://127.0.0.1:9005"
export VLM_URL="http://127.0.0.1:8080"
export VLM_MODEL="lfm2.5-vl"

scripts/colab_real_e2e.sh
```

The full script:

1. Installs system build tools and Rust if missing.
2. Builds `ggml-org/llama.cpp`.
3. Starts `llama-server` with `LiquidAI/LFM2.5-VL-450M-GGUF:Q4_0`; llama.cpp downloads the model from Hugging Face on first launch.
4. Reuses the official DPhi-Space/SimSat service from the smoke test, or starts it with Docker Compose if it is not already reachable.
5. Runs `cargo run -- run` against SimSat and the LFM2.5 VLM server.
6. Runs `cargo run -- verify` and checks the expected evidence files exist.

## Expected clean success signal

A successful full run ends with:

```text
SUCCESS: real Colab E2E completed. Evidence packet: .../data/alert_packets/colab-e2e
```

and these files must exist:

- `data/alert_packets/colab-e2e/manifest.json`
- `data/alert_packets/colab-e2e/alert.json`
- `data/alert_packets/colab-e2e/vlm_reports/role_01_triage.json`
- `data/alert_packets/colab-e2e/vlm_reports/role_03_narrative.json`

## What this project is doing, simply

Forest Guardian asks SimSat for two Sentinel-like satellite image cutouts of the same place at two different dates. It uses the non-visible spectral bands to measure whether healthy forest turned into bare or disturbed ground. If it finds a meaningful disturbance patch, it sends the before/after RGB images plus the numeric evidence to a small LFM2.5 vision-language model. The model does not make the detection; it helps triage obvious visual risks and writes a readable explanation. The system then writes an evidence packet with the alert, metadata, VLM reports, hashes, and a manifest so the result can be checked later.
