# Issue #876: real Chromium A/B results

Defer the production prepare_emit API/state change: no repeatable direct-run latency benefit, no improvement in the normal checked-first path, and higher editor-only linear-memory high-water size.

## Provenance

- Run: https://github.com/bondiano/flick-lang/actions/runs/45156657215 (success).
- Baseline: `1918f8919368817bac94f8c84244607d68f28557`.
- Candidate and harness: `0ee305261875a85ef3d6e52923080d555aab3d27`.
- Host: AMD EPYC 7763; Linux x64; Chromium 156.0.8078.4; Node 24.18.0.
- Order A1, B1, B2, A2; 3 warm-up edits and 12 measured edits per independent worker session; 24 observations per fixture/scenario/variant.
- Artifact 14810038322: original ZIP SHA-256 `6d7034a81000daaed5fb83a6fb661c409e6108a09cf82981d8e6e3477329fb54` verified against GitHub metadata.
- The 12 complete output payload digests match across all four reports. Unicode diagnostic/hover checks, invalid/fix/empty/restore transitions and both profiles passed.
- All working trees were clean; report revision, harness revision, host, fixture hashes, sample counts and all recomputed aggregate distributions were verified locally.

## Primary direct edit-to-emit result

| Fixture | Before p50 ms | After p50 ms | Delta | A1 to B1 | A2 to B2 |
| --- | ---: | ---: | ---: | ---: | ---: |
| hello | 454.2 | 455.8 | +0.35% | +3.12% | -2.17% |
| classes | 452.3 | 451.7 | -0.13% | +5.33% | -3.35% |
| sales-report | 467.3 | 462.7 | -0.98% | +1.67% | -2.53% |

A negative latency delta is faster. All three direct-run signs reverse between the two paired rounds. Aggregate changes below 1% do not demonstrate a repeatable improvement.

## Every measured scenario

| Fixture | Scenario | Before p50 ms | After p50 ms | Delta | Before p95 ms | After p95 ms | Before peak MiB | After peak MiB |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| hello | diagnostics-only | 0.5 | 0.5 | +0.00% | 0.5 | 0.5 | 20.8750 | 21.4375 |
| hello | edit-first-emit | 454.2 | 455.8 | +0.35% | 466.3 | 475.5 | 47.5000 | 47.5000 |
| hello | checked-first-emit | 456.1 | 458.6 | +0.55% | 466.4 | 470.6 | 47.5000 | 47.5000 |
| hello | unchanged-profile | 464.8 | 450.4 | -3.10% | 479.1 | 470.8 | 47.5000 | 47.5000 |
| hello | check-after-direct-run | 0.8 | 0.7 | -12.50% | 0.9 | 0.8 | 47.5000 | 47.5000 |
| hello | large-check-small-run | 447.8 | 447.5 | -0.07% | 467.1 | 456.1 | 47.5625 | 47.5000 |
| classes | diagnostics-only | 4.1 | 4.3 | +4.88% | 5.2 | 6.1 | 20.8750 | 21.4375 |
| classes | edit-first-emit | 452.3 | 451.7 | -0.13% | 461.1 | 472.3 | 47.6250 | 47.5625 |
| classes | checked-first-emit | 450.4 | 464.2 | +3.06% | 467.2 | 472.8 | 47.6250 | 47.6250 |
| classes | unchanged-profile | 459.8 | 456.5 | -0.72% | 484.0 | 468.1 | 47.6250 | 47.5625 |
| classes | check-after-direct-run | 4.8 | 4.9 | +2.08% | 5.6 | 5.9 | 47.6250 | 47.5625 |
| classes | large-check-small-run | 458.6 | 454.4 | -0.92% | 474.0 | 466.0 | 47.6250 | 47.5625 |
| sales-report | diagnostics-only | 5.5 | 5.9 | +7.27% | 8.2 | 8.9 | 20.8750 | 21.4375 |
| sales-report | edit-first-emit | 467.3 | 462.7 | -0.98% | 485.2 | 477.3 | 47.7500 | 47.5625 |
| sales-report | checked-first-emit | 462.1 | 465.7 | +0.78% | 479.1 | 499.0 | 47.7500 | 47.7500 |
| sales-report | unchanged-profile | 467.1 | 465.4 | -0.36% | 477.9 | 479.7 | 47.7500 | 47.5625 |
| sales-report | check-after-direct-run | 6.9 | 6.9 | +0.00% | 8.6 | 7.8 | 47.7500 | 47.5625 |
| sales-report | large-check-small-run | 464.5 | 461.3 | -0.69% | 478.4 | 476.5 | 47.7500 | 47.5625 |

The normal checked-first path is slower at aggregate p50 in every fixture. Direct-run memory reductions are 0 to 0.1875 MiB, while diagnostics-only high-water size rises by 0.5625 MiB in all three fixtures. Peak memory here is WASM linear memory, not browser RSS or JavaScript heap.

## Production WASM payload

| Metric | Before | Candidate | Delta |
| --- | ---: | ---: | ---: |
| Raw bytes | 4,545,296 | 4,546,300 | +1,004 |
| Gzip bytes (level 6) | 1,766,527 | 1,766,428 | -99 |

These byte counts describe the bound production WASM payload, not a complete documentation-site bundle. The existing production size gate passed for both builds.

## Preserved evidence

`before-1.json`, `after-1.json`, `after-2.json` and `before-2.json` contain every observation and the production-worker call timings. `comparison.json` and `environment.txt` are unchanged artifact files. `provenance.json`, `analysis.json` and this summary record source identities, verification and interpretation. `SHA256SUMS` covers these report files; the original ZIP has its separately verified digest in provenance.
