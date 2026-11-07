#!/usr/bin/env bash
# The crossing measurement on both modes, alternating: usage micro.sh <pairs>
cd "$(dirname "$0")/micro"
for i in $(seq 1 "${1:-1}"); do
  echo "== rt $i"; taskset -c 0 node dist/rt/flick-server.js
  echo "== js $i"; taskset -c 0 node dist/js/flick-server.js
done
