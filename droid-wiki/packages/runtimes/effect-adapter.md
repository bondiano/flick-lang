# Effect adapter and compatibility surface

Active contributors: bondiano, Vassiliy Kuzenkov (non-bot authors on `origin/main` for `packages/effect-ts/`).

`@flick-lang/effect-ts` both executes explicitly selected Effect builds and supplies optional capabilities to [owned-runtime](owned-runtime.md) builds. Its bridges translate execution context and resource ownership as well as values; importing the package is not equivalent to making every foreign API portable.

## Package organization

`packages/effect-ts/package.json` pins `effect`, `@effect/platform-node`, and `@effect/platform-browser` to **4.1.2**, depends on the workspace-owned runtime, and declares optional `pg`, React, and React DOM peers.

| Area | Responsibility |
| - | - |
| `packages/effect-ts/src/index.ts` | Generated Effect-build ABI: combinators, causes, layers, owners, schemas, atoms, and standard helpers. |
| `packages/effect-ts/src/effect/` | Export-specific wrappers over Effect families: data, resources, streams/channels, transactions, networking, observability, HTTP, RPC, server functions, and reactivity. |
| `packages/effect-ts/src/bridge*.ts` | Native/Effect interpreter, context, data, collection, layer, and schema conversions. |
| `packages/effect-ts/src/form.ts`, `packages/effect-ts/src/route.ts`, `packages/effect-ts/src/storage.ts` | Schema-validated form drafts and URL codecs, plus browser storage-backed atoms. |
| `packages/effect-ts/src/ui/` | Effect-backed React roots, views, policies, SSR, and hydration. |
| `packages/effect-ts/src/sql/`, `packages/effect-ts/src/httpapi/`, `packages/effect-ts/src/rpc/`, `packages/effect-ts/src/cli/` | Host/application adapters and Flick data-model conversion. |
| `packages/effect-ts/src/runner/` | Node, browser, Cloudflare, and development tooling runners. |
| `packages/effect-ts/tools/surface/` | Rust compatibility-manifest linter, currently moved here in the dirty tree. |

`packages/effect-ts/src/capabilities.ts` reports a mismatched Effect version and describes the operation capability matrix. The Node and browser entry runners check the version without retaining every Effect module in browser bundles; Node refuses execution with exit code 2. The lower-level `start` helper and Cloudflare runner do not perform that guard themselves. Tests exercise the larger matrix. Pinning and a passing matrix constrain dependencies but are not a proof that a new Effect release preserves scope or cancellation semantics.

## Effect execution still has Flick owners

Generated Effect builds use cold Effect computations, but `packages/effect-ts/src/owner.ts` adds Flick's lexical ownership discipline on top of native Effect scopes. It owns child/watcher sets, foreign promises, closing hooks, supervisor defects, and a resource scope.

Its C0–C6 sequence mirrors the [owned kernel](owned-runtime.md#owners-resources-and-cancellation): close admission, run hooks, interrupt/await children and supervisors, settle foreign work, then release resources last-first. `inside` supplies both `CurrentOwner` and Effect's `Scope`; `withOwner` masks close while restoring the body's interruptibility. Replacing these helpers with generic detached forks would break region/resource contracts even if examples still printed the same output.

`packages/effect-ts/src/concurrency.ts` wraps bounded/unbounded Effect groups with positive-limit validation and typed-failure Result conversion. `packages/effect-ts/src/race.ts` owns race/timeout behavior; these must retain loser cleanup rather than just resolve the winning Promise.

Refreshable resources add another ownership boundary: `packages/effect-ts/src/effect/Resource.ts` uses the holder/lease machinery in `packages/effect-ts/src/effect/lease.ts`. A failed refresh preserves the previous successful acquisition; a successful swap retires the old value only after its final lease ends. Closing the resource's region retires its final value. Releasing immediately on refresh would invalidate overlapping users.

## Crossings are shaped and bidirectional

```mermaid
graph LR
    Foreign["Generated foreign wrapper"] --> Shapes["IR shapes and adapters"]
    Shapes --> Values["Data / callback / opaque conversion"]
    Values --> Bridge["Bridge kernel"]
    Native["Native Ctx and Owner"] --> Bridge
    Bridge --> Host["Effect Context and Scope"]
    Host --> EF["Effect fiber"]
    EF --> Wait["Completion / interruption join"]
    Wait --> Native
```

`flick-core/src/codegen/js/foreign.rs` chooses the import and whether it crosses. `flick-core/src/codegen/js/bridge.rs` emits conversions for function stages, nested lists/records/tuples, Option/Result, effects, and known opaque shapes such as layers, codecs, fibers, durations, schedules, and collections. This is a signature-derived ABI, not arbitrary introspection of any host object.

`packages/effect-ts/src/bridge.ts` is the public generated-code boundary. It initializes late-bound interpreter operations and exports adapters. The important internal split is:

- `packages/effect-ts/src/bridge_kernel.ts`: `fromEffect` runs an Effect call immediately when possible or returns a native wait; `callEffect` runs native callbacks in a native fiber. Interruption crosses in both directions and waits for the other side to finish cleanup.
- `packages/effect-ts/src/bridge_host.ts`: native `Ctx.host` carries Effect context; Effect's context carries native evidence under `KERNEL`. Native service operations are exposed through wrappers, and scopes are attached to owners.
- `packages/effect-ts/src/bridge_context.ts`: propagates fiber-local context across interpreter calls.
- `packages/effect-ts/src/bridge_data.ts`, `packages/effect-ts/src/bridge_collections.ts`, `packages/effect-ts/src/bridge_layer.ts`, and `packages/effect-ts/src/bridge_schema.ts`: preserve the relevant data/opaque identities and callback semantics.

One documented ordering limit remains in `packages/effect-ts/src/bridge_host.ts`: the shared host owner is attached at the first crossing's position on the native owner's finalizer stack, not separately at every host acquisition. Each crossing's own scope preserves its local order. Do not describe this as unrestricted global interleaving of every acquisition.

Foreign JS values outside trusted runtime packages are also subject to copy adaptation in codegen. A bridge conversion does not replace foreign mutation isolation or a declared cancellation/error-mapping policy.

## Fallback tables and UI coherence

The current package-owned tables are:

| Source | Meaning |
| - | - |
| `packages/effect-ts/compiler-bindings.json` | Routing, aliases, core builtins, and the UI family that must share an implementation, compiled by `flick-core/build.rs`. |
| `runtime/rt_exports.txt` | Actual built native export inventory. |
| `runtime/rt_stubs.txt` | Root exports still identical to pure-core throwing stubs. |
| `packages/effect-ts/rt_bridge_stubs.txt` | 38 module/export fallback or ABI-override rows. |
| `packages/effect-ts/shared_ui_exports.txt` | 43 inert UI exports required to have identical identity on both sides. |

`tools/rt-exports.mjs` generates the first two text inventories and bridge-stub inventory from built exports and explicit overrides. A bridge row is not necessarily “unimplemented”: redaction, formatting, arbitrary-generation integration, and tracer ownership can deliberately prefer Effect's ABI even when a native operation exists.

Reachable adapter-only UI operations cause `flick-core/src/build/bridge_select.rs` to select `AvailableWithUi`. It follows source references through builtin definitions and includes source-library exports, not just one entry call. That moves opaque atom/view/registry values together; shared text, element, fragment, and component constructors remain on the native side when safe.

`packages/effect-ts/test/rt-bridge-ui-shared.test.ts` checks constructor identity against `packages/effect-ts/shared_ui_exports.txt`. Mixing otherwise similar native and Effect registries without this family rule is not a supported shortcut.

Native omissions include streamed server UI and serializable atom helpers. `packages/effect-ts/src/ui/index.ts` hydrates initial and streamed atom-state scripts into the adapter registry and observes later scripts for SSR roots. `packages/effect-ts/src/ui/server.ts` provides the adapter server path; native server rendering alone does not imply native streaming/state parity.

## Host adapters and boundaries

`packages/effect-ts/src/host.ts` defers Node-only modules until needed. Browser/worker hosts have no Node filesystem, process arguments, environment, or signals. Separate RPC client/server modules avoid pulling Node server imports into client-only code; their generated imports still need a suitable host and dependencies.

Node uses the signal/cleanup entry runner. Browser execution reports failures to the console and supports development replacement. The Cloudflare runner creates an owner per fetch, supplies Worker bindings as a service, and returns 500 on failure; `wait_until` explicitly delegates background work to the host request context.

The SQL adapter in `packages/effect-ts/src/sql/pg.ts` ties pools to provide scopes and cancels interrupted PostgreSQL statements before discarding their connections. Its current CancelRequest helper is plain TCP; a TLS-only cancel endpoint can force reliance on query timeout/connection termination. Cancellation does not roll back prior writes outside a transaction.

## Effect compatibility is recorded, not inferred

`stdlib/Effect/` still exists, but it contains Effect compatibility evidence, not the active Flick standard-library source:

- `stdlib/Effect/snapshot.json` records the pinned release's entrypoints, symbols, and fingerprints.
- `stdlib/Effect/surface/` assigns wrapped, language-provided, excluded, or pending decisions.
- `packages/effect-ts/tools/surface/tests/surface/mod.rs` parses/lints those decisions.
- `packages/effect-ts/tools/surface/tests/coverage.rs` resolves wrapped targets through compiler interfaces, checks reviewed foreign-binding provenance, and checks use by `tests/run/stdlib/` fixtures.

The workspace crate is `flick-effect-surface`; `just effect-coverage` runs its manifest test and prints the generated report under `target/tmp/`. `just effect-bump <version>` drives `tools/dts-importer/` to refresh release evidence; review must resolve the resulting differences. A `pending` or excluded classification is not implemented support, and Effect-surface coverage is not native-runtime parity.

## Modification and checks

For a new wrapper, update the Flick declaration, adapter implementation, reviewed provenance/surface decision where relevant, runtime routing, and independent execution tests. Test both a direct Effect build and an RT crossing when supported. Exercise callbacks, cancellation, resources, errors, and opaque value round trips, not just scalar return values.

Useful checks are `pnpm -C packages/effect-ts typecheck`, `pnpm -C packages/effect-ts lint`, `pnpm -C packages/effect-ts test`, `cargo test -p flick-effect-surface --test coverage`, `just effect-coverage`, and the [runtime-wide checks](index.md#changing-execution-behavior). Tests are present under `packages/effect-ts/test/rt-bridge-*.test.ts`; no suite was run merely to write this wiki.

## Key source files

| Source | Responsibility |
| - | - |
| `packages/effect-ts/src/capabilities.ts` | Exact release guard and capability matrix. |
| `packages/effect-ts/src/owner.ts` | Flick lexical ownership on Effect. |
| `packages/effect-ts/src/bridge_kernel.ts` | Interpreter crossings and cancellation propagation. |
| `packages/effect-ts/src/bridge_host.ts` | Context/scope ownership and ordering limit. |
| `packages/effect-ts/src/ui/index.ts` | Initial/streamed atom hydration and DOM mounting. |
| `packages/effect-ts/src/sql/pg.ts` | PostgreSQL ownership and cancellation. |
| `packages/effect-ts/tools/surface/tests/coverage.rs` | Release evidence linked to real declarations/tests. |
| `packages/effect-ts/tools/surface/tests/surface/mod.rs` | Surface manifest model and linter. |

See [libraries](../libraries.md) for public Flick modules and [plugins](../plugins.md) for syntax/schema/UI generation.
