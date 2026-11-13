# Backend extraction baseline

This inventory describes the native emitters and their callers before the CLI
switches to the [component contract](contract.md). It records dependencies that
must cross that boundary as plain data. It is not an IR 1.0 freeze or evidence
that the official emitters already run as components.

The [environment inventory](targets.md) records the Node, browser and Go
boundaries. The [numeric review](numeric-policy.md) confirms the existing
ordinary/checked/unsafe policy and identifies its independent boundary tests.
The [GADT row contract](../formal/gadt-rows.md) records the E10 transition
on top of E11 type congruence. This still does not freeze IR 1.0 or complete
any of the missing component inputs below.

## Data used by the existing build

| Consumer | Current input and owner | Component boundary |
| --- | --- | --- |
| Shared lowering | Checked modules, imports and lang declarations | Versioned `Program`, including every declared lang item |
| Shared release optimization | Export visibility, generic bodies and arities from own modules and built-in interfaces | Portable transformations must retain their explicit facts; these facts are not implied by optional expression type names |
| JS module emission | `codegen::Options`: built-in names, package identities, resolved foreign specifiers, runtime mode, bridge availability and labels | Module layout, foreign bindings and versioned target facts/options |
| JS release ABI | Worker exports and direct instance methods derived from reference lowering and published interfaces | Backend-owned calculation from plain export/ABI facts; no callback to the checker |
| JS initialization | `RuntimeLayout` over all own modules, prefix exports and dependencies | Complete initialization graph from IR, including physical prefix modules |
| TypeScript emission | `Typing::prepare`, separately for every physical module | `flick.typescript@0.1.0` projection: declarations, annotations, imports and imported module names |
| Public `.d.ts` and Promise facades | Shared interface/projection rules, finalized ABI summaries and foreign type forms | Shared compiler outputs remain reserved; a backend receives needed plain projections |
| Published `.iface` | `build::interface`, finalized dependencies and ABI fingerprints | Shared compiler ownership; explicit interface inputs for external modules |
| JS source maps | Generated mappings, source text, line/UTF-16 columns and paths relative to each output file | Source manifest plus bytes and line starts; the backend must preserve physical-module locations |
| JS built-in closure | Emitted imports and, with declarations, every built-in module | Declared built-in IR and source inputs; the component cannot load compiler globals |
| JS launchers | Selected entry, defining export, optional runner, runtime mode and schema AOT | Explicit roots/runner facts and launch descriptions |
| JS foreign shipping | Library ownership, resolved `.mjs`/`.cjs`/declaration files and duplicate/symlink checks | Host-resolved, content-addressed inputs; no guest filesystem lookup |
| JS dependencies | Project runtime/adapter versions, npm resolution and conflicts | Host-owned pinned runtime inputs and dependency validation |
| Go emission | `codegen::go::Input`: own/built-in IR, entry or suites, source paths/line starts, foreign Go files, binding tables and lang items | Whole-program request with declared sources, foreign/binding inputs and explicit roots |
| Go assembly | Selected package/runtime sources, `go.mod`, `go.sum` and output listing | Binary artifacts and declared runtime dependencies |
| Go toolchain | Host `go build`, its environment and executable naming | Pinned host tool adapter and validated graph; a guest never starts Go |
| Schema AOT | Host imports the generated entry and runs its decoder compiler | Explicit host step and inputs; it is not part of a pure guest generation call |
| Build cache | Sources, effective config, dependency interfaces, compiler/runtime inputs and selected roots | Include backend id/version/digest, every declared side input, environment, profile and tool/runtime pins; revalidate cached results |

The entry points are `flick/src/commands/build.rs` (`artifacts`),
`flick-core/src/build/javascript/{mod,module}.rs` (`emit`, `compiled_module`, `builtins`) and
`flick-core/src/build/go.rs` (`emit`). Build/run/test reuse those paths; their
existing cache, package and CLI behavior must survive the extraction.

`Program` contains neither complete typing/effect facts nor the checker's UI
metadata. Receiving its optional `Expr::ty`/`Param::ty` names does not provide
either analysis. A required extra projection needs an identified schema,
version and content input. Missing data must be rejected rather than inferred
from prelude spellings or supplied through a checker callback.

Built-in checks start with separate source maps. Lowering now combines them
into a bundle with distinct file identities and the exact text used during
lowering, including generated declarations. The host cache retains both the
IR and those texts; legacy IR-only entries are regenerated. Preparing a
component request explicitly remaps that bundle into the project's source
table and supplies built-in bodies with `builtin` module metadata. The same
declared source inputs govern validation, diagnostics and source maps.

## Portable transformations and target representations

`flick-core/src/opt` currently serves JS and Go through `build::optimized`.
Its transformations include dictionary specialization/devirtualization,
inlining and beta reduction, known-constructor branches, literal folding,
constant/alias propagation, sequence fusion, temporary aggregate elimination
and removal of unused pure code. Their permission comes from Flick's
evaluation and numeric contracts, rather than from the eventual target syntax.

Those permissions retain effects and termination: no task execution or
memoization, merged layers, service lookup moved across `provide`, discarded
runtime call, arithmetic reassociation or unconditional `x * 0` folding.
`opt::walk` distinguishes expressions that may be dropped from computations
that may throw or perform work. Structural IR validation alone proves none of
these transformations equivalent.

JS worker/curry conventions, direct-method exports, generator/runtime
wrappers, object/constructor layouts, import syntax and TypeScript source
forms belong to the JS backend. Go's native scalar selection, boxing,
closure ownership, Go layout and private target analyses belong to Go. Shared
helpers are extracted only when both implementations actually depend on them;
having similar names does not make target representations portable.

The names `opt::fold::js_number` and `json_string` need particular care. Their
results implement observable `Show`/`Display` behavior and are also consumed
by `flick-model/src/native/show.rs`. They are not merely JavaScript source
printers. Moving or renaming them must preserve that shared language behavior.
JS-specific literal/source spelling is a separate concern.

## Numeric behavior to preserve

The existing numeric policy remains the baseline during extraction. In
particular, ordinary integer operators are not replaced by checked or unsafe
APIs.

| Contract | JS path | Go path |
| --- | --- | --- |
| `Int` is signed 32-bit; add/subtract/multiply wrap | Bitwise coercion and `Math.imul` | Native `int32` arithmetic/runtime helpers |
| Integer division truncates toward zero; remainder has the dividend's sign | `divInt` / `remInt` | Flick runtime integer helpers |
| Ordinary integer division/remainder by zero give zero | Runtime helpers and shared `numeric::int_arith` folding | Flick helpers, not an unchecked Go division |
| `minInt / -1` wraps to `minInt`; its remainder is zero | Runtime/folding contract | Same wrapping contract |
| Integer powers wrap; negative exponents retain the existing truncating result | `powInt` / shared folding | Flick integer power helper |
| Float signed zero, NaN, infinities and rounding remain observable where specified | JS number/runtime behavior | Matching Flick runtime behavior, not algebraic reassociation |
| Structural numeric equality uses `SameValueZero` | Runtime `equals` | Flick structural equality |
| BigInt arithmetic and failure cases retain the language/runtime contract | Explicit runtime helpers where needed | Flick BigInt helpers |
| Checked and unsafe integer APIs keep their distinct failure behavior | Checked/unsafe runtime functions | Checked/unsafe runtime functions |

The implementation anchors are `flick-core/src/numeric.rs`,
`flick-core/src/opt/fold.rs`, `flick-core/src/codegen/js/binary.rs`,
`flick-core/src/codegen/go/arith.rs`, `runtime/src/core/text.ts` and
`runtime-go/flickrt/{numbers,effnum}.go`. Numeric folding tests independently
pin overflow, division, remainder, exponent and float-formatting boundaries.
Language/runtime fixtures are still required in debug and release on both
targets; agreement between two emitters is not an independent oracle.

## Validation scope

The contract probes in `flick-backend/tests/consumers.rs` exercise JS/TS/maps
and Go with IR and plain inputs, without constructing `Compilation` or
`Settings`. They establish that initial boundary, not coverage of every item
in the inventory above. The literal component tests prove native/component
transport and independently expected Unit/binary output in both modes.

For the bounded-host baseline at PR #356, `just check`, `just wasm-plugins` and all 98
tests selected by `just e2e-tests` passed. That run is not the historical
2,544-build prelude comparison and does not replace official-component
conformance, locked/offline invalidation tests, browser-loader gates or
before/after measurements on the same real project.
