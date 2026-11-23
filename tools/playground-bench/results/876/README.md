# #876: retain the current lazy emission strategy

The production session and compiler worker are retained. The tested
`prepare_emit` candidate removed a repeated frontend pass for a direct build
of changed text, but did not deliver a repeatable browser latency benefit.
Its diagnostics-only WASM memory capacity also increased. This change keeps
the regression coverage, benchmark, raw evidence and reproducible candidate
patch instead of adding that API and state to production.

This is the measured-defer outcome permitted by
[issue #876](https://github.com/bondiano/flick-lang/issues/876). The experiment
is preserved in
[commit 0ee3052](https://github.com/bondiano/flick-lang/commit/0ee305261875a85ef3d6e52923080d555aab3d27)
and the [direct-emission patch](../../direct-emit.patch). The temporary
measurement workflow was removed after its results were collected.

## What was tested

The existing session performs a cached editor analysis on a changed `update`,
invalidates its emitted revision, and leaves backend generation until Run.
The first emit creates a full analysis with typed IR. Later emits or profile
changes for the same revision reuse that analysis; backend generation still
runs for each request. The IDE worker remains separate.

The candidate changed only the direct-build boundary. A worker build of text
that had not already been checked called `prepare_emit`: one full analysis
served emission and subsequent queries, avoiding the preceding editor-only
analysis. It cleared the previous editor cache to avoid retaining an older
checked graph. Checks alone still used the existing lazy emission path.

The playground's normal refresh checks before emission. A build after a
completed check therefore remained a separate control: the candidate did
not reuse that editor analysis as typed IR. Both that path and a subsequent
check after a direct Run were measured, along with a large checked revision
followed by a smaller direct build. No broad persistent typed-IR cache was
introduced.

## Browser experiment and provenance

The primary comparison ran the real compiler worker in initialized Chromium
workers, using uninstrumented optimized production WASM. Each fixture and
scenario received a fresh worker, three warm-up edits and twelve measured
edits. The two variants ran in A-B then B-A order, yielding 24 observations
per fixture/scenario/variant. Startup is excluded from latency; memory peaks
include initialization, warmups and the untimed preparation in each scenario.

- Baseline A: `1918f8919368817bac94f8c84244607d68f28557`.
- Candidate B and harness: `0ee305261875a85ef3d6e52923080d555aab3d27`.
- Host: AMD EPYC 7763, Linux x64, Chromium `156.0.8078.4`, Node `24.18.0`.
- Build: Rust `1.95.0`, wasm-bindgen `0.2.130`, Binaryen `133`; opt-level `z`,
  fat LTO, debuginfo stripping and the existing WASM deduplication recipe.
- Fixtures: the shipped `hello`, `classes` and `sales-report` examples. Their
  source hashes, worker hashes, WASM hashes and raw observations are recorded.
- Successful [Actions run 45156657215](https://github.com/bondiano/flick-lang/actions/runs/45156657215)
  on 2026-11-23; original artifact ZIP SHA-256
  `6d7034a81000daaed5fb83a6fb661c409e6108a09cf82981d8e6e3477329fb54`.

Both builds completed before sampling. The experiment compared exact source
commits, not a PR merge ref. The full provenance is in
[browser/provenance.json](browser/provenance.json), and
[browser/environment.txt](browser/environment.txt) contains tool versions.
All pooled distributions were independently recomputed from the four raw
reports. No optional timing hooks were present in these browser binaries.

## Direct edit to first emit

These are worker request times in milliseconds. A negative delta is faster.
The last two columns compare each A/B pair separately; B ran before A in the
second pair.

| Fixture | A p50 | B p50 | Pooled delta | A1 to B1 | A2 to B2 | A p95 | B p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| hello | 454.2 | 455.8 | +0.35% | +3.12% | -2.17% | 466.3 | 475.5 |
| classes | 452.3 | 451.7 | -0.13% | +5.33% | -3.35% | 461.1 | 472.3 |
| sales-report | 467.3 | 462.7 | -0.98% | +1.67% | -2.53% | 485.2 | 477.3 |

All three per-pair signs reverse. The pooled median differences are below
one percent in magnitude and do not establish a repeatable improvement.
The page-to-worker round-trip medians likewise change from 456.1 to 457.1 ms,
453.6 to 453.8 ms and 468.5 to 464.3 ms respectively. Raw min/p50/p95/max and
every individual observation are preserved; these small samples are not a
claim about a population tail or a statistical significance test.

## Checks, controls and memory

| Scenario | hello A → B p50 ms | classes A → B p50 ms | sales-report A → B p50 ms |
| --- | ---: | ---: | ---: |
| Diagnostics-only changed text | 0.5 → 0.5 | 4.1 → 4.3 | 5.5 → 5.9 |
| First emit after completed check | 456.1 → 458.6 | 450.4 → 464.2 | 462.1 → 465.7 |
| Unchanged revision, debug → release | 464.8 → 450.4 | 459.8 → 456.5 | 467.1 → 465.4 |
| Check an edit after direct Run | 0.8 → 0.7 | 4.8 → 4.9 | 6.9 → 6.9 |
| Large check, then smaller direct Run | 447.8 → 447.5 | 458.6 → 454.4 | 464.5 → 461.3 |

The normal checked-first path has no pooled median improvement in any
fixture. The unchanged-profile control performs no new frontend analysis in
either variant, so its measured variation should not be attributed to the
removed frontend pass. The full 18-case p50/p95/memory table is in
[browser/summary.md](browser/summary.md).

| Scenario | hello A → B MiB | classes A → B MiB | sales-report A → B MiB |
| --- | ---: | ---: | ---: |
| Diagnostics-only peak | 20.8750 → 21.4375 | 20.8750 → 21.4375 | 20.8750 → 21.4375 |
| Direct first-emit peak | 47.5000 → 47.5000 | 47.6250 → 47.5625 | 47.7500 → 47.5625 |

The candidate increases diagnostics-only linear-memory capacity by 0.5625
MiB in every fixture, while its direct-build peak reduction ranges from zero
to 0.1875 MiB. These are WASM linear-memory high-water capacities from
`memory.buffer.byteLength`, not live allocations, retained Rust heap, browser
RSS or JavaScript heap. They do not establish the cause of the allocation
difference. Together with the absent repeatable latency gain, they do not
justify adopting this candidate.

## Experimental WASM payload cost

| Metric | Baseline A | Candidate B | Delta |
| --- | ---: | ---: | ---: |
| Browser WASM bytes after bindgen | 4,545,296 | 4,546,300 | +1,004 |
| Gzip bytes, level 6 | 1,766,527 | 1,766,428 | -99 |

Both builds passed the existing 4,600,000-byte gate on the optimized artifact
before bindgen. The table describes the final browser WASM from the
experiment, not the complete documentation-site bundle. The final change
retains production source behavior and adds tests, tools and evidence; a
separate final bundle delta is not claimed.

## Separate Node stage attribution

For attribution, [stages.patch](../../stages.patch) was applied in an isolated
worktree containing the candidate. Legacy and candidate worker dispatches
then drove the **same instrumented WASM**. This is not an independently built
baseline and not browser acceptance data. The local host used Node
`24.22.3`/V8 `13.6.233.17-node.63`, separate from the browser experiment's Node
version. Five measured edits followed three warmups for `hello`, with no
competing local build during sampling.

| Direct changed-source request scope | Legacy dispatch p50 ms | Candidate dispatch p50 ms |
| --- | ---: | ---: |
| Sum of exclusive frontend scopes per observation | 1.434 | 0.776 |
| Parse | 0.171 | 0.075 |
| Resolve/check, exclusive | 1.152 | 0.673 |
| `lower_if_clean` scope | 0.048 | 0.030 |
| Backend | 702.819 | 662.262 |
| Output-map construction | 0.924 | 0.957 |
| JSON string serialization | 4.165 | 4.105 |

Every measured direct-build request enters parse and resolve/check twice
with legacy dispatch and once with candidate dispatch. The lowering marker
wraps its eligibility guard; an entry does not prove that a full IR lowering
occurred. Frontend totals sum exclusive scopes within each observation
before taking the median; individual stage medians are not additive.

The measured frontend median difference is about 0.66 ms. The backend
accounts for about 98.5% of worker time in each dispatch. Backend code is
identical between these dispatches, and its separate variation is much
larger than the removed frontend work. Marker overhead and timer resolution
also affect small scopes. These results locate repeated work; they are not a
production speedup estimate.

The checked-first control still enters parse/check once during its timed
build; unchanged-profile enters neither. Diagnostics-only invokes no backend.
Both direct-build runs reach 47.5 MiB of linear-memory capacity. Exact stage
counts, distributions, tool versions and hashes are in
[node/comparison.json](node/comparison.json) and
[node/environment.json](node/environment.json); raw observations are in
[node/legacy.json](node/legacy.json) and
[node/candidate.json](node/candidate.json).

## Correctness and retained coverage

All twelve complete output-payload digests agree across the four Chromium
reports, covering debug/release success, edit, invalid input, fix, empty text
and restoration. Validation checks JS, declarations, complete file maps,
runner entry and diagnostics; failure must clear prior output. Diagnostic
and hover ranges are checked after a non-BMP prefix in UTF-16 units. The Node
stage experiment independently passed the same parity checks.

The retained Rust regressions compare persistent sessions with fresh cold
runs across these revision transitions and a module rename/removal of its
runner. They check lazy emission, analysis reuse across profiles and unchanged
queries, and release of the previous emitted analysis on a changed check.
The experimental candidate-specific assertions were adapted to the retained
public `update`/`emit` behavior.

## Reproduction and evidence integrity

The [harness README](../../README.md) documents matched builds, both worker
sources, the six scenarios and optional instrumentation. For the historical
experiment, use the exact A/B commits above and its
[pinned workflow](https://github.com/bondiano/flick-lang/blob/0ee305261875a85ef3d6e52923080d555aab3d27/.github/workflows/playground-876-bench.yml).
The candidate and instrumentation patches can also be applied to an isolated
checkout for new experiments; a later base is a new comparison.

`direct-emit.patch` SHA-256 is
`4c43966e14ce7846fbd33048d1eb40f39c214281942e4e85725ceb21c522a3dd`;
`stages.patch` SHA-256 is
`b0bc6cbb91d313b4c3a34d6695ba2d9d95f4444e89ea48f99c4f17f39e6261b6`.
The candidate patch was checked to reconstruct the measured production
session and worker while retaining test wiring, and the stage patch applies
after it. The browser files are copied unchanged from the verified evidence
bundle, with its [SHA256SUMS](browser/SHA256SUMS). The original artifact ZIP
is not needed to read the committed observations after Actions retention
expires.

Further optimization should first narrow the measured backend cost. This
evidence does not justify retaining more typed IR during edit-only requests
or adding the tested direct-emission state to production.
