# Forest Guardian MVP

Forest Guardian is now a runnable SimSat + LFM VLM evidence pipeline. The implementation intentionally does **not** ship stubs, silent fallbacks, or mock service behavior in the production `run` command: it calls real HTTP services and exits non-zero if SimSat or the LFM VLM endpoint is unavailable.

## Build plan implemented

1. **Real SimSat acquisition boundary**: `run` fetches baseline and current Sentinel-like observations from `GET /data/image/sentinel` with the required bands: red, green, blue, nir, swir16, and swir22.
2. **Deterministic multispectral analysis**: the pipeline computes NDVI, NDMI, NBR, BSI, and NDWI; extracts connected candidate loss patches; scores the largest eligible patch; and maps the score to an alert level.
3. **Required LFM VLM review**: the pipeline sends SimSat RGB imagery and numeric alert evidence to a llama-server-compatible LFM VLM endpoint for triage and narrative roles. If the VLM is unreachable or returns non-JSON, the run fails.
4. **Evidence packet output**: every successful run writes observation metadata, alert JSON, feature summary, VLM role reports, a Markdown report, file hashes, and a manifest.
5. **Demonstrable checks**: unit tests exercise real index computation, patch extraction, cloud blocking, and evidence-packet verification without using production stubs.

## Expected services

Start SimSat on port 9005 and llama-server with an LFM VLM model on port 8080, for example:

```bash
llama-server \
  -hf LiquidAI/LFM2-VL-1.6B-GGUF:Q8_0 \
  --jinja \
  --port 8080 \
  --temp 0.0 \
  --seed 42
```

LFM2-VL or LFM2.5-VL are both acceptable as long as the server exposes the OpenAI-compatible `/v1/chat/completions` endpoint and supports image inputs.


## Runtime validation for the required smallest LFM2.5 VLM

The smallest non-negotiable LFM2.5 VLM target is `LiquidAI/LFM2.5-VL-450M-GGUF:Q4_0`. It is the compact 450M vision-language model and the Q4_0 GGUF artifact is the smallest official llama.cpp quantization target documented for local execution.

Use this script on a machine with Hugging Face access and recent llama.cpp multimodal support:

```bash
scripts/check_lfm25_vl_runtime.sh
```

The script prefers the official llama.cpp Hugging Face invocation:

```bash
llama-mtmd-cli -hf LiquidAI/LFM2.5-VL-450M-GGUF:Q4_0   -p "Describe this image as JSON with keys image_quality and notes."   --n-predict 64
```

If `llama-mtmd-cli` is not installed, the script downloads `LFM2.5-VL-450M-Q4_0.gguf` into `models/lfm2.5-vl-450m/` and refuses to report success if the artifact is implausibly small.

## Run the pipeline

```bash
cargo run -- run \
  --simsat-url http://localhost:9005 \
  --vlm-url http://localhost:8080 \
  --vlm-model lfm2-vl \
  --lon -60.025 \
  --lat -3.125 \
  --baseline-timestamp 2026-01-01T00:00:00Z \
  --current-timestamp 2026-02-01T00:00:00Z \
  --out-dir data/alert_packets/latest
```


## Google Colab one-command real E2E

For a clean Colab run that builds llama.cpp, downloads/runs the smallest LFM2.5 VLM target, checks a real SimSat service, runs Forest Guardian, and verifies the evidence packet, use:

```bash
scripts/colab_real_e2e.sh
```

When running from a fresh Colab notebook, use the cells in `colab/FOREST_GUARDIAN_COLAB.md`.
> Colab note: this Codex workspace has no configured GitHub remote, so there is no repo URL I can pre-fill. Either push this branch to GitHub and set `FOREST_GUARDIAN_REPO` to that real URL, or upload a zip of the repo using the documented Colab upload option. Do not paste placeholders such as `<you>` or `YOUR_GITHUB_USER` into bash.
The first runnable cell starts and checks the official `DPhi-Space/SimSat` simulation; the second cell builds/runs LFM2.5-VL and runs the full Forest Guardian evidence pipeline.

## DPhi-Space/SimSat smoke test

To test only the official SimSat simulation in Colab before downloading/running the VLM, use:

```bash
scripts/colab_dphi_simsat_smoke.sh
```

This clones `https://github.com/DPhi-Space/SimSat.git`, installs/starts Docker if needed, runs `docker compose up -d --build`, starts the simulation through the dashboard command API, and saves a Sentinel RGB PNG smoke-test output.

## Real E2E check

After SimSat and the LFM VLM server are running, execute the production pipeline and verify the resulting packet with:

```bash
scripts/run_real_e2e.sh
```

This script does not start fake services. It probes the configured endpoints, runs `cargo run -- run`, verifies the packet, and fails if any required evidence file is missing.

## Verify generated evidence

```bash
cargo run -- verify --packet-dir data/alert_packets/latest
```

## SimSat JSON contract

The dependency-free client expects SimSat to return JSON with this shape for `return_type=array`:

```json
{
  "source": "simsat",
  "image_datetime": "2026-02-01T00:00:00Z",
  "width": 512,
  "height": 512,
  "cloud_cover": 0.05,
  "footprint_geojson": { "type": "Polygon", "coordinates": [] },
  "rgb_png_base64": "...",
  "red": [0.1],
  "green": [0.1],
  "blue": [0.1],
  "nir": [0.7],
  "swir16": [0.2],
  "swir22": [0.2]
}
```

Each band array must contain `width * height` reflectance values normalized to `[0, 1]`.

## What this project does in simple language

Forest Guardian compares two SimSat Sentinel-like images of the same location. It uses multispectral bands such as near-infrared and shortwave-infrared to detect whether healthy forest appears to have become disturbed or bare ground. It then asks an LFM2.5 vision-language model to review the RGB before/after evidence and write a clear narrative, without letting the model invent a higher alert level. Finally, it writes an evidence packet with the alert, metadata, VLM reports, hashes, and a manifest so someone can inspect and verify the result.
