#!/usr/bin/env bash
# tiny mode launcher for walrus testnet modes
set -euo pipefail

MODE="${1:-}"
case "$MODE" in
  testnet-local)  ENV_FILE=".env.testnet-local"  ;;
  testnet-public) ENV_FILE=".env.testnet-public" ;;
  localnet)       ENV_FILE=".env.localnet"       ;;
  *)
    echo "[!] unknown mode: ${MODE}"
    echo "usage: $0 {testnet-local|testnet-public|localnet}"
    exit 1
    ;;
esac

[ -f "$ENV_FILE" ] || { echo "[!] missing $ENV_FILE"; exit 1; }

ENV_OUT=".env"
ENV_BAK=".env.bak.$(date +%s)"

touch "$ENV_OUT"
cp "$ENV_OUT" "$ENV_BAK"

# --- merge only keys present in ENV_FILE into .env ---
merge_env() {
  local preset_file="$1" out_file="$2"
  # read preset line-by-line; accept lines like:
  #   KEY=VAL
  #   export KEY=VAL
  # ignore comments/blank lines
  while IFS= read -r line || [ -n "$line" ]; do
    # strip leading/trailing spaces
    line="${line#"${line%%[![:space:]]*}"}"
    line="${line%"${line##*[![:space:]]}"}"
    # skip comments/blank
    [[ -z "$line" || "$line" =~ ^# ]] && continue
    # strip optional 'export '
    line="${line#export }"
    # must contain '='
    [[ "$line" != *"="* ]] && continue

    key="${line%%=*}"
    val="${line#*=}"

    # trim key whitespace
    key="${key%"${key##*[![:space:]]}"}"
    key="${key#"${key%%[![:space:]]*}"}"

    # escape sed-sensitive chars in value (| &)
    val_esc="$(printf '%s' "$val" | sed -e 's/[&|]/\\&/g')"

    if LC_ALL=C grep -Eq "^[[:space:]]*(export[[:space:]]+)?${key}=" "$out_file"; then
      # replace existing key line
      # macOS sed needs empty string after -i
      sed -E -i '' "s|^[[:space:]]*(export[[:space:]]+)?${key}=.*|${key}=${val_esc}|" "$out_file"
    else
      printf '%s=%s\n' "$key" "$val" >> "$out_file"
    fi
  done < "$preset_file"
}

merge_env "$ENV_FILE" "$ENV_OUT"
echo "[i] merged $ENV_FILE -> $ENV_OUT (backup: $ENV_BAK)"

# source merged env for this run
set -a; source "$ENV_OUT"; set +a

# sanity
command -v walrus >/dev/null || { echo "[!] walrus CLI not found in PATH"; exit 1; }

# defaults (can be overridden in your .env.*)
AGG_BIND="${AGG_BIND:-127.0.0.1:31415}"
PUB_BIND="${PUB_BIND:-127.0.0.1:31416}"
DAEMON_BIND="${DAEMON_BIND:-127.0.0.1:31415}"
SUBWALLET_DIR="${PUBLISHER_WALLETS_DIR:-$HOME/.config/walrus/publisher-wallets}"
N_CLIENTS="${N_CLIENTS:-1}"
SUBWALLET_MIN_BALANCE="${SUBWALLET_MIN_BALANCE:-300000000}"
GAS_REFILL_AMOUNT="${GAS_REFILL_AMOUNT:-150000000}"
WAL_REFILL_AMOUNT="${WAL_REFILL_AMOUNT:-150000000}"

AGG_LOG="${AGG_LOG:-/tmp/walrus-agg.log}"
PUB_LOG="${PUB_LOG:-/tmp/walrus-pub.log}"
DAEMON_LOG="${DAEMON_LOG:-/tmp/walrus-daemon.log}"

WALRUS_PACKAGE_ID="${WALRUS_PACKAGE_ID:-0xd84704c17fc870b8764832c535aa6b11f21a95cd6f5bb38a9b07d2cf42220c66}"

# cleanup handler
cleanup() {
  [ -n "${AGG_PID:-}" ] && kill "${AGG_PID}" 2>/dev/null || true
  [ -n "${PUB_PID:-}" ] && kill "${PUB_PID}" 2>/dev/null || true
  [ -n "${DAEMON_PID:-}" ] && kill "${DAEMON_PID}" 2>/dev/null || true
}
trap cleanup EXIT

echo "[i] MODE: ${MODE}"
echo "[i] updated .env (only keys from ${ENV_FILE})"

case "$MODE" in
  testnet-local)
    mkdir -p "$SUBWALLET_DIR"
    
    echo "[i] starting walrus daemon @ ${DAEMON_BIND}"
    walrus daemon --bind-address "${DAEMON_BIND}" \
      --sub-wallets-dir "$SUBWALLET_DIR" \
      --n-clients "${N_CLIENTS}" \
      --sub-wallets-min-balance "${SUBWALLET_MIN_BALANCE}" \
      --gas-refill-amount "${GAS_REFILL_AMOUNT}" \
      --wal-refill-amount "${WAL_REFILL_AMOUNT}" \
      >"${DAEMON_LOG}" 2>&1 & DAEMON_PID=$!

    echo "[i] logs: ${DAEMON_LOG}"
    ;;
  testnet-public)
    echo "[i] starting walrus aggregator @ ${AGG_BIND}"
    walrus aggregator --bind-address "${AGG_BIND}" >"${AGG_LOG}" 2>&1 & AGG_PID=$!
    
    echo "[i] logs: ${AGG_LOG}"
    ;;
  localnet)
    echo "[i] localnet: only .env updated. No Walrus services started."
    echo "    Start your Sui node separately (e.g., sui start --with-faucet)."
    ;;
esac

echo "[i] press Ctrl+C to stop"
# keep background services alive
while true; do sleep 5; done
