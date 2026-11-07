#!/usr/bin/env bash
# Runs each case of tests/run/stdlib built with --rt (or the MODE given) against its expected stdout.
# usage: stdlib-cases.sh [rt|js] [dir under tests/run]...; writes target/spike-cases/<mode>.tsv
root=$(cd "$(dirname "$0")/../../.." && pwd)
mode=${1:-rt}; shift
export FLICK_STD_PATH=$root/packages/std MODE=$mode ROOT=$root
out=$root/target/spike-cases; mkdir -p "$out/$mode"
one() {
  case=$1; rel=${case#"$ROOT"/tests/run/}; id=${rel%.fl}; id=${id//\//_}; dir=$ROOT/target/spike-cases/$MODE/$id
  rm -rf "$dir"; flag=; [ "$MODE" = rt ] && flag=--rt
  if ! "$ROOT/target/e2e/flick" build "$case" --out-dir "$dir" --profile debug $flag > "$dir.build" 2>&1; then
    printf '%s\tbuild-failed\t%s\n' "$rel" "$(grep -m1 -E '^error' "$dir.build" | cut -c1-140)"; return; fi
  timeout 40 node "$dir/flick-main.js" > "$dir.out" 2> "$dir.err"; code=$?
  want=${case%.fl}.exit; wantcode=0; [ -f "$want" ] && wantcode=$(tr -d ' \n' < "$want")
  if cmp -s "$dir.out" "${case%.fl}.stdout" && [ "$code" = "$wantcode" ]; then printf '%s\tok\n' "$rel"
  elif [ "$code" = 124 ]; then printf '%s\ttimeout\t\n' "$rel"
  else printf '%s\tfailed\t%s\n' "$rel" "$(grep -m1 -E 'Error|error|not ' "$dir.err" | cut -c1-140)"; fi
}
export -f one
dirs=("$@"); [ ${#dirs[@]} = 0 ] && dirs=(stdlib)
for d in "${dirs[@]}"; do find "$root/tests/run/$d" -name '*.fl' ; done | while read -r f; do [ -f "${f%.fl}.stdout" ] && echo "$f"; done \
  | xargs -P 6 -I{} bash -c 'one "$@"' _ {} | sort > "$out/$mode.tsv"
cut -f2 "$out/$mode.tsv" | sort | uniq -c
