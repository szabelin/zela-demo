#!/bin/bash
# Load test for leader_routing endpoint
# Blasts endpoint from 10 threads, counts successes for rpc vs precomputed modes

set -e

# Configuration
CLIENT_ID="${CLIENT_ID:-rpk-2c31aba939754d08a54064d60fdd5df5-2aaaf0c8c7614d7a8bdf661b9429045f}"
CLIENT_SECRET="${CLIENT_SECRET:-H8tG7RtxDmqnkWDYs7PduDWUwLHuhJ00}"
METHOD="${METHOD:-zela.deploy-leader-routing-RPC-v1#e409a0be04c7e9c39bc90b32d6827cf099581100}"
CALLS_PER_THREAD="${CALLS_PER_THREAD:-10}"
THREADS=10

# Get auth token
echo "Getting auth token..."
TOKEN=$(curl -s \
  -u "${CLIENT_ID}:${CLIENT_SECRET}" \
  -d 'grant_type=client_credentials' \
  -d 'scope=zela-executor:call' \
  'https://auth.zela.io/realms/zela/protocol/openid-connect/token' | jq -r '.access_token')

if [ "$TOKEN" == "null" ] || [ -z "$TOKEN" ]; then
  echo "Failed to get auth token"
  exit 1
fi
echo "Token obtained."

# Temp files for results
RPC_RESULTS=$(mktemp)
PRECOMPUTED_RESULTS=$(mktemp)
trap "rm -f $RPC_RESULTS $PRECOMPUTED_RESULTS" EXIT

# Worker function
blast_endpoint() {
  local mode=$1
  local thread_id=$2
  local results_file=$3

  for i in $(seq 1 $CALLS_PER_THREAD); do
    start_ms=$(python3 -c 'import time; print(int(time.time()*1000))')
    result=$(curl -s --max-time 10 \
      -H "Authorization: Bearer $TOKEN" \
      -H 'Content-Type: application/json' \
      -d "{ \"jsonrpc\": \"2.0\", \"id\": 1, \"method\": \"$METHOD\", \"params\": {\"mode\": \"$mode\"} }" \
      'https://executor.zela.io' 2>/dev/null || echo '{}')
    end_ms=$(python3 -c 'import time; print(int(time.time()*1000))')
    latency=$((end_ms - start_ms))

    if echo "$result" | jq -e '.result.slot' > /dev/null 2>&1; then
      echo "success,$latency" >> "$results_file"
    else
      echo "error,$latency" >> "$results_file"
    fi
  done
}

export -f blast_endpoint
export TOKEN METHOD CALLS_PER_THREAD

echo ""
echo "=== Testing RPC mode ($THREADS threads x $CALLS_PER_THREAD calls) ==="
start=$(date +%s)
for t in $(seq 1 $THREADS); do
  blast_endpoint "rpc" $t "$RPC_RESULTS" &
done
wait
rpc_duration=$(($(date +%s) - start))

echo ""
echo "=== Testing PRECOMPUTED mode ($THREADS threads x $CALLS_PER_THREAD calls) ==="
start=$(date +%s)
for t in $(seq 1 $THREADS); do
  blast_endpoint "precomputed" $t "$PRECOMPUTED_RESULTS" &
done
wait
precomputed_duration=$(($(date +%s) - start))

echo ""
echo "============================================"
echo "              RESULTS SUMMARY"
echo "============================================"
echo ""

# RPC results
rpc_success=$(grep -c "^success" "$RPC_RESULTS" 2>/dev/null || echo 0)
rpc_error=$(grep -c "^error" "$RPC_RESULTS" 2>/dev/null || echo 0)
rpc_total=$((rpc_success + rpc_error))
if [ $rpc_total -gt 0 ]; then
  rpc_rate=$(awk "BEGIN {printf \"%.1f\", $rpc_success/$rpc_total*100}")
  rpc_avg_latency=$(awk -F',' '{sum+=$2; count++} END {printf "%.0f", sum/count}' "$RPC_RESULTS")
else
  rpc_rate="N/A"
  rpc_avg_latency="N/A"
fi

echo "RPC MODE:"
echo "  Total calls:    $rpc_total"
echo "  Success:        $rpc_success"
echo "  Errors:         $rpc_error"
echo "  Success rate:   ${rpc_rate}%"
echo "  Avg latency:    ${rpc_avg_latency}ms"
echo "  Duration:       ${rpc_duration}s"
echo ""

# Precomputed results
pre_success=$(grep -c "^success" "$PRECOMPUTED_RESULTS" 2>/dev/null || echo 0)
pre_error=$(grep -c "^error" "$PRECOMPUTED_RESULTS" 2>/dev/null || echo 0)
pre_total=$((pre_success + pre_error))
if [ $pre_total -gt 0 ]; then
  pre_rate=$(awk "BEGIN {printf \"%.1f\", $pre_success/$pre_total*100}")
  pre_avg_latency=$(awk -F',' '{sum+=$2; count++} END {printf "%.0f", sum/count}' "$PRECOMPUTED_RESULTS")
else
  pre_rate="N/A"
  pre_avg_latency="N/A"
fi

echo "PRECOMPUTED MODE:"
echo "  Total calls:    $pre_total"
echo "  Success:        $pre_success"
echo "  Errors:         $pre_error"
echo "  Success rate:   ${pre_rate}%"
echo "  Avg latency:    ${pre_avg_latency}ms"
echo "  Duration:       ${precomputed_duration}s"
echo ""
echo "============================================"
