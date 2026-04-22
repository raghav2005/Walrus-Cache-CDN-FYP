#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v tmux >/dev/null 2>&1; then
  echo "[!] tmux is not installed. Install with: brew install tmux"
  exit 1
fi

SESSION="${1:-walrus-demo}"
MODE="${2:-testnet-public}"
METRICS_ADDR="${METRICS_ADDR:-127.0.0.1:9002}"
FLOW_STEP_SLEEP="${FLOW_STEP_SLEEP:-6}"
DEMO_BLOB_FILE="${DEMO_BLOB_FILE:-$PWD/.demo_blob_id}"

if tmux has-session -t "$SESSION" 2>/dev/null; then
  echo "[i] session '$SESSION' already exists; attaching..."
  exec tmux attach-session -t "$SESSION"
fi

tmux new-session -d -s "$SESSION" -n demo

: > "$DEMO_BLOB_FILE"

p1="$(tmux display-message -p -t "$SESSION":0.0 '#{pane_id}')"
p2="$(tmux split-window -h -P -F '#{pane_id}' -t "$p1")"
p3="$(tmux split-window -v -P -F '#{pane_id}' -t "$p1")"
p4="$(tmux split-window -v -P -F '#{pane_id}' -t "$p2")"

tmux send-keys -t "$p1" "cd '$PWD' && ./scripts/demo_t1_mode.sh '$MODE'" C-m
tmux send-keys -t "$p2" "cd '$PWD' && METRICS_ADDR='$METRICS_ADDR' CACHE_DB_PATH='./walrus_cache_cdn_indexer' DEMO_BLOB_FILE='$DEMO_BLOB_FILE' ./scripts/demo_t2_indexer_follow.sh" C-m
tmux send-keys -t "$p3" "cd '$PWD' && METRICS_ADDRS='${METRICS_ADDR},127.0.0.1:9003' ./scripts/demo_t3_metrics_watch.sh" C-m
tmux send-keys -t "$p4" "cd '$PWD' && sleep 4 && METRICS_ADDR='127.0.0.1:9003' CACHE_DB_PATH='./walrus_cache_cdn_demo' DEMO_BLOB_FILE='$DEMO_BLOB_FILE' DEMO_STEP_MODE='manual' STEP_SLEEP='$FLOW_STEP_SLEEP' ./scripts/demo_t4_cache_flow.sh 'prof-demo'" C-m

tmux select-layout -t "$SESSION":0 tiled

echo "[i] launched tmux session '$SESSION'"
echo "[i] pane 1: mode.sh ($MODE)"
echo "[i] pane 2: index --follow"
echo "[i] pane 3: metrics watch"
echo "[i] pane 4: deterministic cache flow demo (manual Enter per step)"
echo "[i] demo blob tracking file: $DEMO_BLOB_FILE"
echo "[i] detach: Ctrl+b then d"

tmux attach-session -t "$SESSION"
