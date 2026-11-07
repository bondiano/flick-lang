#!/usr/bin/env bash
# Alternating load runs of two server builds, A B A B …, as tools/perf runs tools/load.
# usage: load.sh <label> <A server.js> <B server.js> <path with {id}> <expect> <runs> [env for both]
# Writes a TSV line per run to target/spike-load/<label>.tsv: mode, run, rps, p50, p95, p99, rss before soak, rss peak in soak, VmHWM, load average, errors, server CPU µs per answer.
root=$(cd "$(dirname "$0")/../../.." && pwd)
label=$1; a=$2; b=$3; route=$4; expect=$5; runs=$6; shift 6
mkdir -p "$root/target/spike-load"; out="$root/target/spike-load/$label.tsv"; : > "$out"
port=${PORT_BASE:-3941}
one() { # mode server run
  mode=$1; server=$2; run=$3
  PORT=$port taskset -c 0-3 env NODE_ENV=production CLIENT_JS=${CLIENT_JS:-} node "$server" > "$root/target/spike-load/$label.$mode.$run.server" 2>&1 &
  pid=$!
  for i in $(seq 1 200); do curl -s -o /dev/null -m1 "http://127.0.0.1:$port${route//\{id\}/warm}" && break; sleep 0.1; done
  log="$root/target/spike-load/$label.$mode.$run.log"
  (cd "$root" && taskset -c 4-7 node tools/load/index.mjs --url "http://127.0.0.1:$port$route" --expect "$expect" --levels 16 --warmup 5 --run 5 --runs 3 --soak 10 --idle 0 --workers 3 --pid $pid) > "$log" 2>&1
  hwm=$(awk '/VmHWM/{print $2/1024}' /proc/$pid/status)
  ticks=$(awk '{print $14+$15}' /proc/$pid/stat)
  answers=$(sed -nE 's/answers ([0-9]+);.*/\1/p' "$log")
  cpu=$(awk -v t="$ticks" -v a="$answers" -v hz="$(getconf CLK_TCK)" 'BEGIN{printf "%.0f", t/hz*1e6/a}')
  kill $pid; wait $pid 2>/dev/null
  rps=$(awk -F'|' '/\| median \| 16/{gsub(/ /,"",$4); print $4}' "$log"); p50=$(awk -F'|' '/\| median \| 16/{gsub(/ /,"",$5); print $5}' "$log"); p95=$(awk -F'|' '/\| median \| 16/{gsub(/ /,"",$6); print $6}' "$log"); p99=$(awk -F'|' '/\| median \| 16/{gsub(/ /,"",$7); print $7}' "$log")
  before=$(sed -nE 's/.*before soak ([0-9.]+),.*/\1/p' "$log"); peak=$(sed -nE 's/.*peak ([0-9.]+),.*/\1/p' "$log"); errors=$(sed -nE 's/.*errors: (.*)/\1/p' "$log")
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$mode" "$run" "$rps" "$p50" "$p95" "$p99" "$before" "$peak" "$hwm" "$(cut -d' ' -f1 /proc/loadavg)" "$errors" "$cpu" | tee -a "$out"
}
for run in $(seq 1 "$runs"); do one A "$a" "$run"; one B "$b" "$run"; done
