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
