# Forest Guardian Real Colab E2E with DPhi-Space/SimSat

These are the Colab commands to first prove the official DPhi-Space/SimSat simulation is running, then run the full Forest Guardian pipeline with the smallest LFM2.5 VLM through llama.cpp.

## Cell 1 — SimSat simulation smoke test

This starts the official `DPhi-Space/SimSat` repository with Docker Compose, starts the satellite simulation through the dashboard API, then downloads one Sentinel RGB image.

```bash
%%bash
set -euo pipefail
cd /content
rm -rf forest-disturbance
git clone https://github.com/<you>/forest-disturbance.git forest-disturbance
cd forest-disturbance

scripts/colab_dphi_simsat_smoke.sh
```

Expected final line:

```text
SUCCESS: SimSat smoke test completed. Outputs in /content/simsat_smoke_outputs
```

Expected output files:

- `/content/simsat_smoke_outputs/current_position.json`
- `/content/simsat_smoke_outputs/sentinel_rgb.png`

> Note: the official SimSat quick start uses `docker compose up`. If your Colab runtime does not provide Docker, run SimSat on another machine and set `SIMSAT_URL` to that API URL.

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
