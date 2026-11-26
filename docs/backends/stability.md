# Backend ABI and IR stability decision

This document records the explicit stability decision for the
[`flick:backend` component contract](contract.md) and the portable typed IR
(`flick-ir`), the semantic and architectural obligations required before a
`1.0.0` freeze, the plain-data fact inventory across backend consumers, and the
separation between binary build outputs / tool DAGs and the text plugin ABI.

## 1. Decision: `flick:backend@0.1.0` and IR `0.19.0` remain `0.x`

- **`flick:backend` stays at `0.1.0`.** The WIT package version in
  `flick-backend/wit/backend.wit` and `flick_backend::VERSION` MUST remain
  `0.1.0` until the semantic prerequisites below are resolved and explicitly
  promoted.
- **`flick-ir` stays at `0.19.0`.** `flick_ir::VERSION` MUST remain `0.x` and
  MUST NOT silently advance to `1.0.0`.
- **Independent version axes.** `flick:backend` (component ABI), `flick-ir`
  (portable IR schema), language version, runtime ABIs (`flickrt`,
  `flick.kernel`), value ABIs (`flick.javascript-ffi`, `flick.go-ffi`, etc.),
  and projection schemas (`flick.typescript@0.2.0`, `flick.javascript@0.1.0`,
  `flick.go@0.1.0`) are versioned independently and checked in the static
  handshake (`flick_backend::handshake`) before `check-target` or `generate`.
- **0.x compatibility rule.** Before `1.0.0`, any IR change that an older reader
  could misread bumps `flick-ir` minor (`0.19.0` -> `0.20.0`), and descriptors
  declare explicit half-open `[minimum, maximum_exclusive)` semver ranges
  (never implicitly accepting prereleases). Any breaking WIT change bumps
  `flick:backend` minor (`0.1.0` -> `0.2.0`).

## 2. Semantic obligations before IR `1.0.0` / `flick:backend@1.0.0`

| Obligation | Status | Contract, anchors and remaining work |
| --- | --- | --- |
| **E10: GADT effect and capture row indices** | **Merged** | Constructor results fix and relate service, error and capture rows (`docs/formal/gadt-rows.md`, `formal/Formal/GadtRowExamples.lean`, `tests/check/{pass,fail}/types/gadt_rows.fl`, `tests/run/codegen/gadt_rows.fl`, `Language.GadtRows`). |
| **E11: Parameter-preserving formal rows and handlers** | **Open (`#433`)** | Type congruence (`docs/formal/type-congruence.md`), `Formal/IndexedRows.lean`, and `Formal/InstanceHandlers.lean` establish indexed row elements and single-instance handlers; remaining parameter-preserving formal row and handler obligations tracked in `#433` must close before freezing IR semantics. |
| **E15a / E15b: Row subtraction** | **E15a accepted; E15b (`#444`) not selected** | Row subtraction remains on the accepted E15a design (`Row.less G`, name-closed removal in `docs/formal/type-congruence.md` and `tests/run/effect/row_without.fl`). E15b (`#444`) was evaluated and not selected; no IR representation change is planned for E15b. |
| **E16 / T2a: Numeric & evaluation-order baseline** | **Baseline pinned** | Ordinary wrapping fixed-width integers (`Int8`..`Int64`, `UInt8`/`Byte`..`UInt64`, `Int`/`Int32`), IEEE 754 `Float`/`Float32`, `BigInt`, total `SameValueZero` / `Ord` (NaN last), and left-to-right evaluation order are specified in [`numeric-policy.md`](numeric-policy.md) and anchored by `flick-backend-support/src/numeric.rs`, `formal/Formal/FixedWidth.lean`, `tests/language/test/Language/{Ints,BigInts,Floats,Equality,NumericBoundaries,FixedWidth}.fl`, and `tests/run/codegen/{ints,bigints,fixed_width,apply_order,eval_order,stage_order}.fl`. Any future backend must preserve these independent expectations in debug and release. |
| **M1–M4: Memory ownership IR and unboxed aggregates** | **Model/Lean complete; IR integration open (`#744`, `#463`)** | Explicit ownership, Perceus RC insertion, `Arena s`, FBIP reuse and Lean balance/preservation proofs are established in [`docs/formal/memory.md`](../formal/memory.md) (`flick-ir/src/memory.rs` is currently a separate representation, not a field of `flick_ir::Program`). Integrating memory ownership IR and unboxed aggregate representations (`#744`, `#463`) into the portable backend contract remains required before `1.0.0`. |
| **SW1: Portable UI metadata** | **Open (`#496`)** | Checker UI component/reactivity metadata is omitted from `flick_ir::Program`. Portable UI consumers (`#496`) need an explicit, versioned plain-data schema or coordinated IR extension rather than ad-hoc checker queries. |
| **Complete foreign dependency closure** | **Partially open** | JS library `.mjs`/`.cjs` shipping currently over-approximates across compilation-wide libraries (`flick.javascript-ffi@0.1.0`), and Go uses `flick.go-ffi@0.1.0` plus `go/bindings/` resource inputs. Complete per-binding dependency closure and transitive ecosystem package pinning across targets remain required before `1.0.0`. |

## 3. How memory IR and portable UI metadata affect IR versions and compatibility

Evolution of memory lowering (M1–M4, unboxed aggregates) and portable UI
metadata (SW1) follows two separate compatibility mechanisms so backend
components are never silently broken:

1. **Changes to `flick_ir::Program` bump `flick-ir::VERSION` (not `flick:backend`).**
   - Because `BuildRequest.program` carries CBOR bytes whose header names
     `ir_version` (validated equal to `TargetRequest.ir_version`), adding
     ownership/unboxed-aggregate nodes or required semantic fields to `Program`
     bumps `flick-ir` minor (`0.19.0` -> `0.20.0`, etc.).
   - `flick:backend@0.1.0` does **not** need a WIT bump when `Program` evolves:
     the handshake checks `Descriptor.ir` against `TargetRequest.ir_version`
     before `check-target` or `generate` runs, cleanly refusing components built
     for an older IR version.
   - `flick-backend-support::layout` already derives monomorphic data, record
     and tuple layouts from `IrType` in IR `0.19.0` + a target `PrimitiveTable`;
     once unboxed aggregate hints or Perceus ownership IR become portable inputs
     for Wasm/Native backends, they can either ship as a coordinated `flick-ir`
     minor bump or as a side input (below) depending on whether every consumer
     must understand their semantics.
2. **Consumer-specific facts use versioned `Projection` schemas (`InputKind::TargetFacts`).**
   - Portable UI metadata (`#496`) or an optional sidecar memory-IR projection
     that only specific backends consume can cross `flick:backend@0.1.0` via
     `BuildRequest.projections` (`Projection { module, schema: Abi { name, version }, input }`)
     and `Descriptor.projections`, without changing the WIT world or forcing
     unrelated backends (e.g., `flick/go` or `flick/lua`) to decode UI metadata.
   - Consumers MUST check the exact projection schema name and version before
     decoding and MUST refuse missing or incompatible projections rather than
     guessing or falling back.

## 4. Plain-data consumer fact inventory

Every backend consumes only the `BuildRequest` envelope, `Program` CBOR, and
content-addressed `inputs.read` entries. No backend may construct `Compilation`
or `Settings` or invoke checker callbacks.

| Consumer | Versioned snapshot / schema IDs | Roots & compilation mode | Interfaces & foreign inputs | Source origins & provenance |
| --- | --- | --- | --- | --- |
| **JS / TS (`flick/js`)** | `flick-ir` `0.19.0`; `flick.javascript@0.1.0`; `flick.typescript@0.2.0` (when `typescript = true`); `flick.javascript-ffi@0.1.0`; runtime ABIs `flickrt@0.2.0`, `flick.kernel@0.1.0` | `WholeProgram` (`module_root = none`; entries, `test_roots`, `exports`) and `PerModule` (`module_root = some(own_module)`; emits owned module + needed builtins, never sibling own or external outputs) | `ModuleInput.interface` (`InputKind::Interface`) for `External` modules; `BuildRequest.foreign` + `InputKind::Foreign` shipped library files; `BuildRequest.inline_foreign`; `BuildRequest.declared_dependencies` | `BuildRequest.sources` aligned 1:1 with `Program.files` (own + remapped built-in lowering texts) and `line_starts`; UTF-16 source-map columns computed in guest from declared UTF-8 bytes |
| **Go (`flick/go`)** | `flick-ir` `0.19.0`; `flick.go@0.1.0`; `flick.go-ffi@0.1.0`; runtime ABIs `flickrt@0.2.0`, `flick.kernel@0.1.0` | `WholeProgram` (one Go module per entry or test suite) and `inspect` (`inspect.module` / `module_root`) | `BuildRequest.foreign` (`flick.go-ffi@0.1.0` companion `.go` inputs); `InputKind::Resource` binding tables under `go/bindings/`; `[target.go.dependencies]` via `packages` | `BuildRequest.sources` + `go-projection.line-path` for `//line` directives; `line_starts` for diagnostics and line mapping |
| **Lua preview (`flick/lua`)** | `flick-ir` `0.19.0`; `flick.check-target@0.1.0` descriptor capability; pinned host profiles (`backends/lua/profiles/`) | `WholeProgram` (`entries`, `test_roots`, `exports`, or library exports) | `foreign` global/module declarations validated against pinned API snapshots (`api/*.txt`); `backends/lua/kernel.txt` | `BuildRequest.sources` + `line_starts` for v3 `.map` source maps and `FLICK-P012` use-site spans (including built-in reachability notes) |
| **Memory / native / Wasm consumers (M1–M4)** | `flick-ir` `0.19.0` (`IrType` + `Module.types` + `flick-backend-support::layout`); future coordinated `flick-ir` `0.20+` or versioned memory-IR side input for Perceus RC / `Arena` / FBIP / unboxed aggregates | `WholeProgram` or `PerModule` with explicit `ModuleRole` ownership and closed module graph | Explicit `ModuleInput.interface` for external modules; `descriptor.value-abi` (`FLICK-F013`/`FLICK-F014`) + `Retention` (`call`/`owner`) for foreign handles | `BuildRequest.sources` + `Span` validated against declared UTF-8 source bytes |
| **Portable UI consumers (SW1)** | `flick-ir` `0.19.0` + future versioned `flick.ui@0.x` `Projection` (`InputKind::TargetFacts`) | Root entries / client targets (`target = "client"`) and reachable UI component graph | Explicit host environment tags (`js/browser`, custom `[host]` packages) and `root_handlers` | `BuildRequest.sources` + exact component/template spans |

### Compilation mode invariants

- **`WholeProgram` mode:** `module_root` MUST be `none`. `entries`, `test_roots`,
  `root_handlers`, and `exports` stay inside the closed module graph. Artifacts
  and step outputs MUST NOT collide with any `External` module's declared
  `output` path (or its portable/case/HFS+ aliases).
- **`PerModule` mode:** `module_root` MUST be `some(name)` naming an `Own`
  module in `request.modules` (and `program.modules`). Artifacts and step
  outputs MUST NOT collide with any other `Own` module's or any `External`
  module's declared `output` path.
- Both `WholeProgram` and `PerModule` request/result shapes are pinned by
  reviewable golden snapshots in `flick-backend/tests/schema.rs` alongside the
  canonical `flick:backend@0.1.0` WIT golden.

## 5. Binary artifacts and tool DAGs vs. the text plugin ABI

The `flick:backend` contract is intentionally separate from the `flick:plugin`
ABI:

| Dimension | `flick:backend@0.1.0` | `flick:plugin` |
| --- | --- | --- |
| **Primary payload** | Opaque binary `Artifact` (`bytes: list<u8>`) tagged with `ArtifactKind` (`source`, `source-map`, `declaration`, `object`, `executable`, `resource`, `metadata`). Binary round-trips retain NUL and non-UTF-8 bytes without text conversion; executables retain executable permissions on Unix. | Source blocks, checker/call hooks, IR rewrites, and text emit hooks (`hooks = ["emit"]` sees source/declaration/metadata text while opaque object, executable and resource artifacts keep their raw bytes). |
| **External tool execution** | Declarative `ToolchainStep` DAG (`id`, `dependencies`, pinned `toolchain`, `arguments`, `environment`, `inputs`, `outputs`). Validated topologically (cycles, duplicate IDs, undeclared producers/inputs, unapproved toolchains or env vars, and layout ownership violations are rejected before any step runs). Executed by the host in an isolated staging directory under explicit project `[tools]` grants (`FLICK-P051`/`FLICK-P050`), with executable SHA-256 rechecked before every step. | Plugins never spawn arbitrary build-graph toolchains inside code generation; plugin `fs-read`/`net`/`env` grants are scoped to `[plugins.<package>]`, and contributed commands use `flick:command/command` under `[tools]` grants. |
| **Publication & caching** | Pure guest calls (`inputs.read` only; no WASI fs/clock/net/process). Reproducibility check (two identical generations) gates cache eligibility; final artifacts and step outputs are committed atomically by `flick/src/publish.rs`. | Plugin hooks participate in compiler checking/lowering/emit passes under host-managed plugin caching. |

See also: [Backend component contract](contract.md),
[Backend extraction baseline](baseline.md),
[Selecting a backend component](registry.md), and
[Numeric behavior required of backends](numeric-policy.md).
