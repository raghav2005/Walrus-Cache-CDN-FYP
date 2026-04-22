#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

export RUST_LOG="${RUST_LOG:-info}"
export METRICS_ADDR="${METRICS_ADDR:-127.0.0.1:9003}"
export CACHE_DB_PATH="${CACHE_DB_PATH:-./walrus_cache_cdn_demo}"
DEMO_STEP_MODE="${DEMO_STEP_MODE:-manual}"
STEP_SLEEP="${STEP_SLEEP:-6}"

DATA="${1:-prof-demo}"
OUT="${2:-/tmp/prof-demo.bin}"

if [[ -t 1 ]]; then
  C_RST='\033[0m'; C_B='\033[1m'; C_CYAN='\033[36m'; C_GREEN='\033[32m'; C_YELLOW='\033[33m'
else
  C_RST=''; C_B=''; C_CYAN=''; C_GREEN=''; C_YELLOW=''
fi

echo -e "${C_B}${C_CYAN}[demo-flow] starting deterministic cache walkthrough${C_RST}"
echo -e "${C_B}[demo-flow] env:${C_RST} METRICS_ADDR=${METRICS_ADDR} CACHE_DB_PATH=${CACHE_DB_PATH}"
echo -e "${C_B}[demo-flow] mode:${C_RST} DEMO_STEP_MODE=${DEMO_STEP_MODE} STEP_SLEEP=${STEP_SLEEP}"
echo -e "${C_B}[demo-flow] note:${C_RST} single-process flow keeps metrics ${C_GREEN}cumulative${C_RST} and easier to explain"
echo -e "${C_B}[demo-flow] outputs:${C_RST} blob-data=${C_YELLOW}${DATA}${C_RST} artifact=${OUT}"
echo

if [ "$DEMO_STEP_MODE" = "manual" ]; then
	AUTO_FLAG=""
else
	AUTO_FLAG="--auto"
fi

exec cargo run --features sui-live -- demo-flow --data "$DATA" --out "$OUT" $AUTO_FLAG --step-sleep-secs "$STEP_SLEEP"
