#!/usr/bin/env bash
set -euo pipefail

METRICS_ADDRS="${METRICS_ADDRS:-127.0.0.1:9002,127.0.0.1:9003}"

if [[ -t 1 ]]; then
  C_RST='\033[0m'; C_B='\033[1m'; C_CYAN='\033[36m'; C_GREEN='\033[32m'; C_YELLOW='\033[33m'; C_RED='\033[31m'; C_MAG='\033[35m'
else
  C_RST=''; C_B=''; C_CYAN=''; C_GREEN=''; C_YELLOW=''; C_RED=''; C_MAG=''
fi

metric_value() {
  local payload="$1"
  local key="$2"
  printf '%s\n' "$payload" | awk -v k="$key" '$1==k {print $2; exit}'
}

fmt_int() {
  local v="${1:-0}"
  if [[ -z "$v" ]]; then v="0"; fi
  printf '%s' "$v"
}

while true; do
  clear
  echo -e "${C_B}${C_CYAN}[metrics dashboard]${C_RST} ${METRICS_ADDRS}"
  echo -e "${C_B}timestamp:${C_RST} $(date '+%Y-%m-%d %H:%M:%S')"
  echo -e "${C_B}legend:${C_RST} misses=${C_YELLOW}origin fetches${C_RST}, hits=${C_GREEN}served from cache${C_RST}, purges=${C_MAG}invalidations${C_RST}"
  echo

  IFS=',' read -r -a addrs <<< "$METRICS_ADDRS"
  for addr in "${addrs[@]}"; do
    url="http://${addr}/metrics"
    echo -e "${C_B}--- ${addr} ---${C_RST}"
    if ! payload="$(curl -fsS "$url" 2>/dev/null)"; then
      echo -e "${C_RED}[waiting]${C_RST} not reachable"
      echo
      continue
    fi

    ram_hits="$(fmt_int "$(metric_value "$payload" "cache_ram_hits")")"
    disk_hits="$(fmt_int "$(metric_value "$payload" "cache_disk_hits")")"
    origin_misses="$(fmt_int "$(metric_value "$payload" "cache_origin_misses")")"
    cache_puts="$(fmt_int "$(metric_value "$payload" "cache_puts")")"
    cache_purges="$(fmt_int "$(metric_value "$payload" "cache_purges")")"
    bytes_served="$(fmt_int "$(metric_value "$payload" "cache_bytes_served")")"
    bytes_written="$(fmt_int "$(metric_value "$payload" "cache_bytes_written")")"

    lat_origin="$(fmt_int "$(metric_value "$payload" "request_latency_ms_count{source=\"origin\"}")")"
    lat_disk="$(fmt_int "$(metric_value "$payload" "request_latency_ms_count{source=\"disk\"}")")"
    lat_ram="$(fmt_int "$(metric_value "$payload" "request_latency_ms_count{source=\"ram\"}")")"

    echo -e "${C_GREEN}hits${C_RST}: ram=${ram_hits} disk=${disk_hits}    ${C_YELLOW}misses${C_RST}: origin=${origin_misses}    ${C_MAG}purges${C_RST}: ${cache_purges}"
    echo -e "puts=${cache_puts} bytes_served=${bytes_served} bytes_written=${bytes_written}"
    echo -e "latency_count origin=${lat_origin} disk=${lat_disk} ram=${lat_ram}"

    if [[ "$origin_misses" != "0" && ( "$disk_hits" != "0" || "$ram_hits" != "0" ) ]]; then
      echo -e "${C_GREEN}[signal]${C_RST} miss->hit path confirmed"
    fi
    if [[ "$cache_purges" != "0" ]]; then
      echo -e "${C_MAG}[signal]${C_RST} invalidation activity observed"
    fi
    echo
  done
  sleep 1
done
