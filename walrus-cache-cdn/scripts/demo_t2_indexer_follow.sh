#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

export RUST_LOG="${RUST_LOG:-info}"
export METRICS_ADDR="${METRICS_ADDR:-127.0.0.1:9002}"
export CACHE_DB_PATH="${CACHE_DB_PATH:-./walrus_cache_cdn_indexer}"
INDEX_DEMO_VIEW="${INDEX_DEMO_VIEW:-true}"
INDEX_EXTRA_ARGS="${INDEX_EXTRA_ARGS:-}"
DEMO_BLOB_FILE="${DEMO_BLOB_FILE:-}"

if [[ -t 1 ]]; then
	C_RST='\033[0m'; C_B='\033[1m'; C_CYAN='\033[36m'; C_GREEN='\033[32m'; C_YELLOW='\033[33m'; C_RED='\033[31m'; C_MAG='\033[35m'
else
	C_RST=''; C_B=''; C_CYAN=''; C_GREEN=''; C_YELLOW=''; C_RED=''; C_MAG=''
fi

echo -e "${C_B}${C_CYAN}[indexer] starting continuous Sui feed${C_RST}"
echo -e "${C_B}[indexer] env:${C_RST} METRICS_ADDR=${METRICS_ADDR} CACHE_DB_PATH=${CACHE_DB_PATH} RUST_LOG=${RUST_LOG}"
echo -e "${C_B}[indexer] watch for:${C_RST} ${C_GREEN}cache purge${C_RST}, ${C_YELLOW}SUI EVENT${C_RST}, ${C_MAG}heartbeat${C_RST}"
echo -e "${C_B}[indexer] mode:${C_RST} INDEX_DEMO_VIEW=${INDEX_DEMO_VIEW} (set to false for full verbose event dump)"
if [[ -n "$DEMO_BLOB_FILE" ]]; then
	echo -e "${C_B}[indexer] demo-blob tracking:${C_RST} ${DEMO_BLOB_FILE}"
fi
echo

INDEX_ARGS=(--follow)
if [[ "$INDEX_DEMO_VIEW" == "true" ]]; then
	INDEX_ARGS+=(--demo-view)
fi
if [[ -n "$INDEX_EXTRA_ARGS" ]]; then
	# shellcheck disable=SC2206
	EXTRA_ARR=($INDEX_EXTRA_ARGS)
	INDEX_ARGS+=("${EXTRA_ARR[@]}")
fi

cargo run --features sui-live -- index "${INDEX_ARGS[@]}" 2>&1 | while IFS= read -r line; do
	tracked_tokens=()
	if [[ -n "$DEMO_BLOB_FILE" && -f "$DEMO_BLOB_FILE" ]]; then
		while IFS= read -r tline; do
			[[ -z "$tline" ]] && continue
			if [[ "$tline" == *"="* ]]; then
				tracked_tokens+=("${tline#*=}")
			else
				tracked_tokens+=("$tline")
			fi
		done < "$DEMO_BLOB_FILE"
	fi

	for token in "${tracked_tokens[@]-}"; do
		if [[ -n "$token" && "$line" == *"$token"* ]]; then
			echo -e "${C_B}${C_CYAN}[DEMO-BLOB match=${token}]${C_RST} $line"
			continue 2
		fi
	done

	case "$line" in
		*"cache purge:"*|*"cache expire purge:"*)
			echo -e "${C_GREEN}[indexer/cache]${C_RST} $line"
			;;
		*"SUI EVENT:"*)
			echo -e "${C_YELLOW}[indexer/event]${C_RST} $line"
			;;
		*"indexer heartbeat:"*)
			echo -e "${C_MAG}[indexer/heartbeat]${C_RST} $line"
			;;
		*"No Walrus events found"*)
			echo -e "${C_RED}[indexer/warn]${C_RST} $line"
			;;
		*)
			echo "$line"
			;;
	esac
done
