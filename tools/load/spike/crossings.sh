#!/usr/bin/env bash
# Crossings of the bridge per request: 1000 sequential requests to each bridged load server.
cd "$(dirname "$0")/../../.."
for pair in "json /item" "ssr /render"; do
  set -- $pair
  FLICK_BRIDGE_COUNT=1 NODE_ENV=production PORT=3991 node tools/load/$1/dist/rt/flick-server.js > /dev/null 2> target/spike-load/count.$1 & pid=$!
  for i in $(seq 1 50); do curl -s -o /dev/null -m1 http://127.0.0.1:3991$2/x && break; sleep 0.2; done
  for i in $(seq 1 999); do curl -s -o /dev/null http://127.0.0.1:3991$2/r$i; done
  kill -INT $pid; wait $pid
  echo "$1, 1000 requests: $(grep bridge target/spike-load/count.$1)"
done
