# Selecting a backend component

A project can select an exact component package for `build`, `run` and
`test`. The compiler must include `flick/wasm-backends`; release builds that
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
are different targets. Without it the compatibility default is `js/node`
for JS/TS output and `go/native` for Go. The existing `js`, `ts`, `go` output
choices remain compatible. The temporary native baseline supports its existing
`js/node` and `go/native` environments and rejects an incompatible explicit
environment before consulting the cache. Other environments require a component.
The JS runtime setting `[build] effect = "rt"`
or `"effect"` remains independent of component identity.

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
validation and two identical independent generations. Only then does the
existing binary writer receive the combined output. Protocol and preflight
failures leave earlier output intact. This does not yet make publication
atomic against every filesystem or host-tool failure.

The result's launch records select runners by artifact id. The host stores
validated launch metadata alongside the artifacts, including on cache hits;
it does not guess `flick-main.js`, `test.js`, an extension or a Go executable
name for a selected component. A runnable entry and a test build require
corresponding launch records. Test reporter, filter, jobs, seed and watch
arguments use the common CLI path.

The initial host runtime adapter resolves Node and pins its exact executable
version and SHA-256. The installed Flick kernel is pinned by its manifest and
sorted `dist` contents. Both pins, the backend pin and component file contents
participate in cache invalidation. Missing kernel builds are not advertised
as usable runtime dependencies. A launch rechecks its runtime executable and runtime dependency pins before
starting the host process. Toolchain build steps are refused until an explicit
host adapter approves and executes them; a component cannot start a process.

## Current coverage and remaining extraction

`tests/backends/literal` is an independently implemented third-party fixture,
not the official JS backend. It supports a pure Unit `main` and a deliberately
small pure-test subset. Its tests execute both passing and failing assertions
through the real test runtime. Unsupported IR is diagnosed before generation.
`just wasm-backends` builds the actual guest and checks SDK/native parity,
CLI selection, build/run/test, binary cache restoration, profiles, offline
operation and target/identity refusals.

The current CLI bridge uses whole-program mode. Builtins are explicit external
interfaces in its initial request; the official emitters still need builtin
IR with correctly remapped source provenance, full foreign resolution and
shared projection inputs. Full typing/effect facts and UI metadata are not
assumed to exist in `Program`. See the [dependency inventory](baseline.md).

Projects without an explicit pin still use the temporary native JS/Go
baseline. Shipping and selecting the official JS component, the browser
loader, generic host tool steps, atomic publication, the Go extraction and
full conformance/performance gates remain required. This integration does
not close R3 or R4 or freeze IR 1.0.
