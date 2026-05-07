# Real E2E Validation Status

This repository now has three validation entrypoints:

- `scripts/colab_dphi_simsat_smoke.sh` starts the official `DPhi-Space/SimSat` simulation with Docker Compose and saves a Sentinel RGB smoke-test image.
- `scripts/check_lfm25_vl_runtime.sh` validates the required smallest LFM2.5 VLM runtime target, `LiquidAI/LFM2.5-VL-450M-GGUF:Q4_0`.
- `scripts/run_real_e2e.sh` / `scripts/colab_real_e2e.sh` run the production CLI against real SimSat and real LFM VLM endpoints, then verify the generated evidence packet.

## Current container result

On this container, a full live E2E could not complete because the required external runtime pieces are not available:

1. `llama-mtmd-cli` is not installed.
2. `llama-server` is not installed.
3. Direct HTTPS access to GitHub and Hugging Face model artifacts returns `403 Forbidden` from the container network proxy.
4. Docker is not available here for starting the official `DPhi-Space/SimSat` Docker Compose stack.
5. SimSat is not running at `http://localhost:9005`.

These are environment blockers, not passing checks. The commands below were attempted and should be rerun in Colab or another machine with GitHub/Hugging Face access, Docker support for SimSat, and enough resources for llama.cpp.

```bash
scripts/colab_dphi_simsat_smoke.sh
scripts/check_lfm25_vl_runtime.sh
scripts/colab_real_e2e.sh
```

A successful real E2E must produce all of these files:

- `data/alert_packets/colab-e2e/manifest.json`
- `data/alert_packets/colab-e2e/alert.json`
- `data/alert_packets/colab-e2e/vlm_reports/role_01_triage.json`
- `data/alert_packets/colab-e2e/vlm_reports/role_03_narrative.json`

and `cargo run -- verify --packet-dir data/alert_packets/colab-e2e` must pass.
