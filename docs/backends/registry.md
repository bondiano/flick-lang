# Selecting a backend component

A project can select an exact component package for `build`, `run` and
`test`. `flick/wasm-backends` is enabled by default; release builds that
enable `wasm-plugins` include it too. A compiler without that feature rejects
the selection with `P013` before reading build or test caches.

```toml
[build]
environment = "js/node"

[build.backend]
id = "example/backend"
version = "1.0.0"
sha256 = "<SHA-256 of the exact component, 64 lowercase hex digits>"
component = "backends/example.wasm"
```

The component path is relative to the project. An explicit absolute path is
also accepted for development. The path is a host configuration value; the
component receives its identity and digest, not filesystem access. Version
ranges, missing fields, unknown fields and malformed digests are errors.

`environment` is an open backend/platform pair: `js/node` and `js/browser`
are different targets. Without it the default is `js/node` for `flick/js` and
`go/native` for `flick/go`. Target selection uses `[build] target = "js" | "go"`
(or `--target <name>` on `build`, `run`, `dev`, `test`, `pack` and `inspect`), and
backend-specific options live in `[target.<name>]` and on the CLI as declared
by the selected backend's `describe` descriptor. Unpinned `js` selects the
official `flick/js` component and `go` selects the official `flick/go`
component, whose `go build` step needs `[tools]` `"flick/go" = ["go"]` (or
`--allow-tool go` outside a project). `flick/lua` is a preview component in
`backends/lua` that is not in the official release index; select it explicitly
with `[build.backend]` and see [Lua host profiles](lua-profiles.md).
User-facing configuration and migration guidance is in
[Backends](../src/content/docs/reference/backends.md).

The compiler holds no component. The official ones are release assets: a tag
`backend-<name>-v<version>` publishes `component.wasm` and `backend.json`, and
adds the component to `backends.json` of the rolling release
`backends-index`, with its digest and the backend API and IR ranges its
descriptor accepts (`.github/workflows/backends.yml`). An unpinned project
takes the backend `[[backend]]` of `flick.lock` pins from the package store
(`<store>/backends/<sha256>.wasm`), downloading it once. Without a lock
entry it takes the newest release in the index that accepts this compiler's
backend API and IR, checks its digest and pins it. `--offline` resolves only
from the lock and the store, `flick install --locked` requires the entry,
and `flick update` moves it to the newest release; a failure is `P056`,
before generation, and no other backend is used instead. A compiler built
from a checkout takes that checkout's `backends/<name>/package` instead, as
does `FLICK_BACKENDS_PATH=<dir>/<name>/package`'s parent; `FLICK_BACKENDS_INDEX`
names another index, a URL or a file. Environment support is
checked against the selected backend before a cache hit is reused.
The JS runtime setting `[target.js] runtime = "rt"`
or `"effect"` remains independent of component identity.
The compiler's JS runtime version check applies to the default component and
an explicit pin matching the distributed JS image. Other components negotiate
their own runtime requirements through the backend ABI.

## Invocation and cache behavior

The driver reads a bounded component file, verifies its digest and descriptor
identity, then uses the separate [backend world](contract.md). An explicit
selection never falls back to native emission. This also applies to a failed
selection when an earlier successful native build is cached.

The host supplies reference IR, original UTF-8 sources and line indexes,
plain interfaces, selected entries/test roots, environment, profile, runtime
options and approved tool/runtime pins. Target optimization happens in the
component. The frontend finalizes and owns published `.iface` output using
the same rules as the native baseline. Guest diagnostics retain their codes
and byte ranges, remapped to the host source table, with the component's
id/version/digest attached.

Successful generation must pass handshake, target preflight, request/result
validation and two identical independent generations. Approved host tool steps
execute under `[tools]` grants (`flick/src/backends/tools.rs`, with SHA-256
rechecks before each step in an isolated staging directory beside the output),
and their declared outputs join the build's artifacts. Only then does atomic
publication (`flick/src/publish.rs`) commit the combined output, stale-file
removals and output manifest as one transaction. Protocol, preflight and
host-tool failures (`FLICK-P013`, `FLICK-P012`, `FLICK-P050`, `FLICK-P051`)
leave earlier published output intact.

The result's launch records select runners by artifact id. The host stores
validated launch metadata alongside the artifacts, including on cache hits;
it does not guess `flick-main.js`, `test.js`, an extension or a Go executable
name for a selected component. A runnable entry and a test build require
corresponding launch records. The shipped JS backend also supports compile-only
builds without Node: it emits sources without a runnable launch, then rebuilds
with an executable pin when Node becomes available. Run and test require Node
for `flick/js` and the built native executable for `flick/go`.
Test reporter, filter, jobs, seed and watch
arguments use the common CLI path.

The host tool and runtime adapters (`flick/src/backends/tools.rs`) resolve
`node`, `go` and `pnpm` on `PATH` and pin each executable's exact version and
SHA-256. Runtime pins cover selected kernel/adapter and declared
project package roots, their package contents and actual resolved dependency
graphs. Resolution follows real Node importers, including nested versions and
symlinks; one launch reuses one canonical package snapshot. These pins, the
backend pin, environment, profile, options and component file contents
participate in cache invalidation across `build`, `run`, `dev` and `test`.
Missing kernel builds are not advertised as usable runtime dependencies. A
launch rechecks its runtime executable and runtime dependency pins before
starting the host process. Toolchain build steps require explicit `[tools]`
grants in `flick.toml` (or `--allow-tool <name>` outside a project) and never
run ungranted (`FLICK-P051`); a guest component itself cannot start a process.

## Current coverage and remaining work

`tests/backends/literal` is an independently implemented third-party fixture,
not an official backend. It supports a pure Unit `main` and a deliberately
small pure-test subset. Its tests execute both passing and failing assertions
through the real test runtime. Unsupported IR is diagnosed before generation.
`just wasm-backends` builds the actual guest and checks SDK/native parity,
reproducible `flick/js`, `flick/go` and `flick/lua` components, CLI selection,
build/run/test, binary cache restoration, profiles, offline operation, host
tool steps, and target/identity refusals.

The CLI bridge uses whole-program mode for `build`/`run`/`dev`/`test` and
supports per-module mode for `inspect`. Builtins are explicit lowered
modules with interfaces and the actual lowering texts, including generated
declarations. Their file identities are remapped into the request's source
table alongside the project's files; cached lowering preserves this provenance.
For JS/TS, the bridge supplies host-resolved foreign specifiers, declared
library file bytes, effective runtime/bridge options and versioned TypeScript
projections (`flick.typescript@0.2.0`, `flick.javascript@0.1.0`) for all
physical files; shared `.d.ts` declarations and Promise facades remain host
outputs. For Go, the bridge supplies `flick.go@0.1.0` projections, sibling
`.go` foreign inputs (`flick.go-ffi@0.1.0`), and `go/bindings/` resource tables.
Complete foreign dependency closure and portable UI metadata remain open. Full
typing/effect facts and UI metadata are not assumed to exist in `Program`. See
the [dependency inventory](baseline.md) and the
[ABI and IR stability decision](stability.md).

Projects without an explicit pin use the distributed `flick/js` component for
JS/TS and the distributed `flick/go` component for Go. The browser component
loader remains separate integration work, and `flick:backend@0.1.0` and IR
`0.19.0` remain `0.x` ([stability.md](stability.md)).

DevTools and schema-AOT integrations run through the same contract: DevTools
uses policy-checked launch `runtime-arguments` and `environment`, and schema
AOT runs as a host `ToolchainStep` under `[tools]` (`node`), rechecking the
pinned Node executable and runtime dependencies before execution and committing
its output atomically with the build. Schema output paths remain reserved for
the compiler. A third-party component cannot grant itself tool authority
without a project `[tools]` grant (`FLICK-P051`).
