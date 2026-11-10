# HTTP load measurements

`index.mjs` sends closed-loop GET requests: each client waits for a complete
response before sending its next request. Every request has a fresh `{id}`;
every `|`-separated `--expect` marker must occur in the decoded response body
with that id. Non-200 status codes, wrong bodies, transport failures and
timeouts count as errors and fail the run.

The default `--client fetch` retains the original measurement path.
`--client http` uses Node's HTTP/1 client with keep-alive and the same GET
headers, timeout and complete-body validation. It decodes gzip, deflate and
Brotli responses. Redirects remain status responses rather than following
them. The load fixtures do not redirect.

`just perf` selects `http` for Go JSON and retains `fetch` for SSR.
Comparative measurements on the same Linux host showed the fetch client
limiting both the Flick Go
JSON server and a plain Go `net/http` reference to about 5,000 requests/s;
the direct client exceeded 59,000 requests/s on both. This is a change in
measurement overhead, not an optimization of the Go runtime. Historical
fetch results and direct-client results should be compared separately.
The performance budgets remain unchanged.

To compare clients, keep the running server, concurrency, worker count,
durations and expected response fixed, and change only `--client`:

```sh
node tools/load/index.mjs --url 'http://127.0.0.1:3930/item/{id}' \
  --expect '"item {id}"' --levels 16 --workers 8 \
  --warmup 5 --run 5 --runs 3 --soak 1 --idle 0 --client fetch
node tools/load/index.mjs --url 'http://127.0.0.1:3930/item/{id}' \
  --expect '"item {id}"' --levels 16 --workers 8 \
  --warmup 5 --run 5 --runs 3 --soak 1 --idle 0 --client http
```

Avoid overlapping measurements with builds or tests. `just perf` writes
measured values to `target/perf.json` and failures to
`target/perf-errors.json`, including partial results if a later stage fails.
Run `pnpm -C tools/load test` for the client and command-line regressions.
