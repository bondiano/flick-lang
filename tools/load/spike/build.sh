#!/usr/bin/env bash
# usage: build.sh <dir under tools/load/spike>...: builds each on both modes, dist/rt and dist/js
root=$(cd "$(dirname "$0")/../../.." && pwd)
export FLICK_STD_PATH=$root/packages/std
for d in "$@"; do
  ( cd "$root/tools/load/spike/$d" && rm -rf dist && "$root/target/e2e/flick" build --rt --out-dir dist/rt 2>&1 | grep -vE "Saving build|^$|───>|^Success" ; "$root/target/e2e/flick" build --out-dir dist/js 2>&1 | grep -vE "Saving build|^$|───>|^Success" ) || true
done
