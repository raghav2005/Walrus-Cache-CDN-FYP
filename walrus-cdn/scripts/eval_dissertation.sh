#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-${ROOT_DIR}/evaluation_output}"
SEED="${SEED:-42}"
SCALE="${SCALE:-1}"

mkdir -p "${OUT_DIR}"

echo "[eval] running suite: out=${OUT_DIR} seed=${SEED} scale=${SCALE}"
(
  cd "${ROOT_DIR}"
  cargo run -- evaluate --out-dir "${OUT_DIR}" --seed "${SEED}" --scale "${SCALE}"
)

if command -v python3 >/dev/null 2>&1; then
  echo "[eval] generating plots"
  if ! python3 "${ROOT_DIR}/scripts/plot_eval.py" --input "${OUT_DIR}" --output "${OUT_DIR}/plots"; then
    echo "[eval] plot generation skipped (install matplotlib: python3 -m pip install matplotlib)"
  fi
else
  echo "[eval] python3 not found; skipped plot generation"
fi

echo "[eval] done: artifacts at ${OUT_DIR}"
