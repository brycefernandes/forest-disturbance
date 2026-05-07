#!/usr/bin/env bash
set -euo pipefail

MODEL_REPO="${MODEL_REPO:-LiquidAI/LFM2.5-VL-450M-GGUF}"
MODEL_QUANT="${MODEL_QUANT:-Q4_0}"
MODEL_FILE="${MODEL_FILE:-LFM2.5-VL-450M-Q4_0.gguf}"
MODEL_DIR="${MODEL_DIR:-models/lfm2.5-vl-450m}"
MODEL_URL="${MODEL_URL:-https://huggingface.co/${MODEL_REPO}/resolve/main/${MODEL_FILE}?download=true}"
MODEL_PATH="${MODEL_PATH:-${MODEL_DIR}/${MODEL_FILE}}"
PROMPT="${PROMPT:-Describe this image as JSON with keys image_quality and notes.}"

mkdir -p "${MODEL_DIR}"

echo "Checking smallest LFM2.5 VLM runtime target: ${MODEL_REPO}:${MODEL_QUANT}"
echo "Expected Q4_0 file: ${MODEL_PATH}"

if command -v llama-mtmd-cli >/dev/null 2>&1; then
  echo "Found llama-mtmd-cli; running official llama.cpp Hugging Face target."
  llama-mtmd-cli -hf "${MODEL_REPO}:${MODEL_QUANT}" -p "${PROMPT}" --n-predict 64
  exit 0
fi

if command -v llama-server >/dev/null 2>&1; then
  echo "Found llama-server but not llama-mtmd-cli. Downloading GGUF for local server/manual launch checks."
else
  echo "Neither llama-mtmd-cli nor llama-server was found; downloading model artifact only."
fi

if [[ ! -s "${MODEL_PATH}" ]]; then
  if ! command -v curl >/dev/null 2>&1; then
    echo "curl is required to download ${MODEL_URL}" >&2
    exit 1
  fi
  echo "Downloading ${MODEL_URL}"
  curl --fail --location --continue-at - --output "${MODEL_PATH}" "${MODEL_URL}"
else
  echo "Model file already exists: ${MODEL_PATH}"
fi

bytes=$(wc -c < "${MODEL_PATH}")
if [[ "${bytes}" -lt 100000000 ]]; then
  echo "Downloaded file is unexpectedly small (${bytes} bytes); refusing to mark runtime ready." >&2
  exit 1
fi

echo "Downloaded ${MODEL_PATH} (${bytes} bytes)."
echo "Install a recent llama.cpp with LFM2.5-VL support, then run:"
echo "  llama-mtmd-cli -hf ${MODEL_REPO}:${MODEL_QUANT} -p '${PROMPT}' --n-predict 64"
