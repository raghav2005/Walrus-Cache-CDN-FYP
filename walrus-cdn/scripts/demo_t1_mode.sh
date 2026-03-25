#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
MODE="${1:-testnet-public}"
exec ./mode.sh "$MODE"
