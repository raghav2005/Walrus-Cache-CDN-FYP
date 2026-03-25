#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

SESSION="${1:-walrus-demo}"

if command -v tmux >/dev/null 2>&1; then
  if tmux has-session -t "$SESSION" 2>/dev/null; then
    tmux kill-session -t "$SESSION"
    echo "[i] killed tmux session '$SESSION'"
  else
    echo "[i] no tmux session named '$SESSION'"
  fi
else
  echo "[i] tmux not installed; skipping session cleanup"
fi

# best-effort cleanup for background walrus services started by demo mode scripts
# ignore failures if processes are not running
pkill -f "walrus aggregator" 2>/dev/null || true
pkill -f "walrus daemon" 2>/dev/null || true

echo "[i] demo processes cleanup attempted"
