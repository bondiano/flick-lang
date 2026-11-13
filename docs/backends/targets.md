# Environment inventory before backend extraction

An environment combines backend and platform. This inventory distinguishes
the current native CLI, the browser compiler and the explicit component path;
it does not claim that the official emitters already ship as components.
`go/native` is the current registry spelling for the Go environment.

## Current support

| Boundary | JS on Node (`js/node`) | JS in a browser (`js/browser`) | Go (`go/native`) |
| --- | --- | --- | --- |
| Language types and operations | Shared parser/checker/IR; JS layout and runtime | Same checker and JS emitter; actual host APIs depend on the browser bundle | Shared checker/IR; reachable unsupported constructs report `P012` during emission |
| Numbers | Int/Float/BigInt rules in the [numeric policy](numeric-policy.md) | Same JS numeric representation and kernel helpers | Matching helpers, including division by zero and Float equality |
| Standard library | RT kernel; optional Effect adapter supplies additional integrations | Browser-compatible kernel/adapter bundle; Node-only APIs are unavailable | `flickrt` plus selected Go package sources and binding tables |
| Host services | Explicit `Host.*` services and live layers; legacy ambient APIs also remain | Browser host capabilities only; no filesystem, Node process or database connection in the playground sandbox | Go runtime providers; explicit layers and external integrations require their implementations |
| Foreign module resolution | npm, relative JS files, globals; declared adapters handle callbacks and result/receiver conventions | Browser-resolvable imports/globals and explicitly bundled dependencies; a Node import is not made portable by typechecking | Known binding tables, or a relative binding with its sibling `.go` file; arbitrary npm/global exports have no automatic Go form |
| Foreign adapter forms | Supports the declared JS receiver, catch, optional, variadic and callback forms | Same JS emission, subject to sandbox and available APIs | Relative foreign `catch`, receiver, optional, variadic and adapted callback forms currently report `P012` |
| Build | Native CLI emits JS or TS modules, `.iface`, optional declarations and source maps | `flick-wasm::Session.emit` emits a single analyzed module and its builtin closure in memory; it is not a project/package build API | Native CLI emits a whole-program Go module per entry/test suite and invokes host `go build` |
| Run and test | CLI uses Node; RT and Effect modes are separate settings | Playground runs a disposable worker; browser tests use the existing browser harness, not CLI `run/test` | CLI uses the generated executable, including test reporter/filter/jobs/seed behavior |
| Source locations | JS/TS source maps and UTF-8 source spans; editor positions use UTF-16 conversion | Playground compiler converts diagnostics/editor offsets to UTF-16; worker reports retain browser source mapping behavior | Generated `//line` directives for own modules; builtin source provenance is not currently merged into that table |
| External tools | Node; package manager for install; TypeScript tools for their relevant checks | Browser and wasm-bindgen compiler bundle; no process spawning from the compiler guest | Go toolchain and module dependencies; a cached pure app can build without downloading unrelated modules |
| Cleanup | Runtime handlers, finalizers and cancellation tested on RT and Effect | Same runtime contract while the worker is alive; terminating the worker is process disposal, not proof of user finalizer execution | Fiber/runtime cleanup tested against the shared fixtures; process termination similarly has host limits |

The current browser compiler statically links the JS emitter. It neither loads
an official backend component nor implements the R3d browser loader. Its
default builtin surface also includes APIs that a browser cannot execute;
the reduced build removes whole Node/server modules, not every unsupported
use. A successful common check is therefore not a promise of browser execution.

`Host.fl` supplies explicit console, clock, randomness, environment and argument
services with replaceable layers. It has not removed the ambient entry points:
for example, `Platform.stdout_line`, `Platform.env` and `Platform.args` still
have empty runtime rows, and `Console`/`Random` retain default services. An
empty runtime row therefore does not establish absence of host access. The
environment preflight cannot rely on service rows alone for those existing
bindings. `tests/run/effect/host_services.fl` checks explicit live/fake layers;
it does not prove that every host API has been migrated.

## Independently specified boundaries

| Boundary | Existing accepted/rejected evidence and implementation anchors |
| --- | --- |
| Shared language behavior | `tests/language/` assertions; `tests/run/portable.txt` selects JS/TS/Go debug/release cases with expected output; `flick-model/tests` checks its supported fragment |
| Effect/RT availability | `flick-core/src/build/capability_tests.rs` accepts supported and unreachable unused bindings, rejects reachable adapter-only bindings at their use, including re-exports and initialization |
| Foreign typing and trust | `tests/check/{pass,fail}/ffi/`, `flick/tests/ffi_forms.rs`, `flick/tests/lsp_editor/foreign.rs`; a `foreign pure` declaration remains the binding author's promise |
| Go foreign emission | `codegen/go/foreign.rs` and `codegen/go/tests.rs` cover mapped and missing bindings; `tests/run/codegen/abi.{fl,go,mjs}` pins a portable local binding |
| Go run/test/cache behavior | `flick/tests/go_main.rs` checks real executables, restoration, exits/signals, matching failure reports and output after cleanup |
| Browser compilation and execution | `flick/tests/wasm_demo.rs`, `docs/test/compiler.test.ts`, `docs/test/loader.test.ts` and `docs/e2e/`; shared invalid programs retain diagnostics, worker isolation is tested separately |
| Browser-only APIs | `tests/check/fail/ui/browser.fl` pins browser/server UI boundaries; those checks are not general package-target preflight |
| Source maps and positions | `flick-core/tests/features.rs`, `flick/tests/unicode_path.rs`, `flick/tests/wasm_demo.rs`, `flick/tests/lsp_editor/`; Go uses the `//line` path in `codegen/go/mod.rs` |
| Component selection | `flick/tests/backends.rs` checks literal third-party build/run/test, profile/cache/offline behavior, version/digest/target rejection and guest diagnostics |

This table names evidence; it is not a claim that every suite was rerun for a
documentation change. Check outcomes belong in the PR that changes behavior.

## Preflight boundary to complete

The registry already parses an environment independently of JS runtime mode.
The temporary native path accepts `js/node` and `go/native`; it refuses an
explicit incompatible environment before cache lookup. The component path
performs descriptor/ABI/capability validation and calls `check-target` before
`generate`. Its initial literal fixture advertises only its small supported
subset. Neither path establishes full official-backend conformance.

Common `check` and IDE analysis currently establish language typing, rather
than complete target support. Go diagnostics can still originate in the
emitter, and explicit native `js/browser` currently fails at selection. That
refusal does not satisfy the browser `Db` acceptance criterion.

The shared target preflight must receive explicit environment root handlers,
package target declarations and required effect/foreign/operation facts. Those
facts cannot be assumed to exist in `Program`'s optional type names. When the
project selects an environment, CLI and IDE should use the same preflight
result and preserve its diagnostic code and source range. General checking
without a selected target must continue to check portable libraries.

In particular, a required `Db` without a browser root handler must be an
unhandled-effect diagnostic on `js/browser`, at the requiring expression;
the same project with its Node handler must build on `js/node`. Package
`targets` and root handlers must use the complete environment, since `js`
alone cannot distinguish those cases. Known unsupported IR and foreign forms
must fail before generation or a host tool starts. Host tool availability
and its approved version/digest remain separate preflight inputs.

The official JS/browser/Go component routes, generic tool steps, atomic
publication and the full target-aware CLI/IDE preflight remain implementation
work. W and N add columns and independently expected fixtures when they are
implemented; they are not currently supported targets.
