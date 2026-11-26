---
title: "Backends"
---

Flick code generation runs through WebAssembly components implementing the `flick:backend@0.1.0` contract rather than hardcoded target code in the CLI. `flick build`, `flick run`, `flick dev`, `flick test`, `flick pack` and `flick inspect` all select, verify and execute the project's backend component before writing or launching any output.

See [Project](../project/) for the full `flick.toml` schema, [Code generation](../codegen/) for JS and Go output details, and [Contributed commands](../commands/) for backend-provided CLI verbs and templates.

## Selecting a target and environment

Without an explicit component pin, Flick selects an official backend from `[build] target` (or `--target <name>` on the CLI):

| Backend | Short name | Default environment | Notes |
| - | - | - | - |
| `flick/js` | `js` (default) | `js/node` | Official JavaScript and TypeScript backend; also supports `js/browser`, `js/bun` and `js/cloudflare`. |
| `flick/go` | `go` | `go/native` | Official Go backend; emits a Go module per entry/test suite and runs `go build` under `[tools] "flick/go" = ["go"]`. |
| `flick/lua` | `lua` | — | Preview component (not in the official release index); select with `[build.backend]` and an explicit environment (`lua/love`, `lua/wow-retail` or `lua/wow-forever`). |

`[build] environment` selects a `<backend>/<host>` pair such as `"js/node"`, `"js/browser"`, `"js/bun"`, `"js/cloudflare"`, `"go/native"`, or a host contributed by a `[host]` library package:

```toml
schema-version = 1

[project]
kind = "app"
entry = "App.Main"

[build]
target = "js"
environment = "js/node"

[target.js]
runtime = "rt"
```

When `[build] target` is omitted and `[build] environment` is set, the backend prefix of `environment` selects the target. Backend-specific options live under `[target.<name>]` and on the CLI as declared by the backend's `describe` descriptor (cached by component SHA-256 so `--help` never reaches the network). Unknown target names report `FLICK-P059`, unknown or invalid `[target.<name>]` keys report `FLICK-P058`, and unsupported environments or hosts report `FLICK-P065`.

### Explicit component pinning (`[build.backend]`)

To use a custom, third-party, or preview backend component, pin its exact identity, version, SHA-256 digest and path in `[build.backend]`:

```toml
[build]
environment = "js/node"

[build.backend]
id = "example/backend"
version = "1.0.0"
sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
component = "backends/example.wasm"
```

- `id` and `version`: exact backend identity and semantic version (version ranges are rejected). They must match the descriptor returned by the component.
- `sha256`: 64 lowercase hexadecimal digits matching the exact bytes of the `.wasm` component.
- `component`: path to the WebAssembly component file, relative to the project root (an absolute path is also accepted for local development). The host reads and verifies the file; the guest component never receives filesystem access.

Missing fields, unknown fields, malformed digests or mismatched component identities fail before generation (`FLICK-P013`).

## Registry, lockfile and offline operation

Official backends (`flick/js` and `flick/go`) are distributed as release assets rather than embedded inside the compiler binary.

1. On `flick new`, `flick install`, or the first unpinned build, the compiler reads the backends index and picks the newest release of the selected backend whose `backend_api` and `ir` ranges accept the running compiler.
2. It downloads `component.wasm`, verifies its SHA-256 digest, stores it at `<store>/backends/<sha256>.wasm` (where `<store>` is `FLICK_STORE`, or `flick/pkg` inside the user's cache directory), and pins it in `flick.lock`:

```toml
[[backend]]
id = "flick/js"
version = "0.2.0"
sha256 = "<64 lowercase hex digits>"
url = "https://github.com/bondiano/flick-lang/releases/download/backend-js-v0.2.0/component.wasm"
```

3. Subsequent `build`, `run`, `dev`, `test` and `inspect` invocations read the pinned digest from `flick.lock` and load `<store>/backends/<sha256>.wasm` without touching the network.

| Flag or environment variable | Behavior |
| - | - |
| `--offline` | Resolves official backends only from `flick.lock` and `<store>/backends/<sha256>.wasm`. Missing lock entries or missing/corrupted store files fail with `FLICK-P056`. |
| `flick install --locked` | Verifies that every required `[[backend]]` entry is already pinned in `flick.lock` and present in the store; fails (`FLICK-P034`) rather than updating the lockfile. |
| `flick update` | Updates `[[backend]]` pins in `flick.lock` to the newest compatible release in the index and fetches them into the store. |
| `FLICK_BACKENDS_PATH` | Directory containing local `<name>/package/{component.wasm,backend.json}` directories (used instead of the release index for local development). |
| `FLICK_BACKENDS_INDEX` | Overrides the backends index with another URL or local JSON file path (takes precedence over checkout/`FLICK_BACKENDS_PATH` lookup). |

Any missing backend, incompatible API/IR range, or SHA-256 mismatch stops with `FLICK-P056` before code generation. The compiler never substitutes a different backend.

## JavaScript runtime modes

On `flick/js`, the JavaScript runtime mode is independent of component identity:

- `[target.js] runtime = "rt"` (default): compiles against Flick's own `@flick-lang/runtime` kernel with no Effect dependency required.
- `[target.js] runtime = "effect"`: compiles through the `@flick-lang/effect-ts` adapter when a project uses Effect-adapter-backed APIs.

Override the configured runtime for a single command with `--runtime`:

```sh
flick build --runtime rt
flick test --runtime effect
flick run --runtime rt
```

Libraries a project depends on are compiled with the consuming project's effective `runtime`. Reachable uses of Effect-adapter-only bindings when `runtime = "rt"` and `@flick-lang/effect-ts` is not installed fail during preflight (`FLICK-P064`) at the use site rather than at runtime.

## Cache invalidation, watch/dev/test and failure isolation

Every backend generation is keyed by the complete `BuildRequest` and host environment:

- Backend identity, version, and component bytes SHA-256 (`[build.backend]` or `[[backend]]`).
- Selected target, `[build] environment`, active tags, `[build] profile` (`debug` or `release`), and all resolved `[target.<name>]` / CLI options (including `runtime`).
- Lowered IR, source files, foreign inputs, projections, and declared dependencies.
- Pinned host runtime executables (such as Node or Go version and SHA-256) and resolved runtime dependency graphs.

Changing any of these inputs invalidates the build, `flick dev` / `--watch` generation, and `.flick/test/` test generation cleanly:

- **Determinism gate:** cache eligibility requires validated, reproducible generation (two identical independent runs of the guest component); non-deterministic results are rejected.
- **No silent fallback:** a failed component load, handshake mismatch, preflight refusal (`FLICK-P012`), transport error (`FLICK-P013`), or ungranted/failing host tool (`FLICK-P051`/`FLICK-P050`) never silently falls back to native emission and never reuses a stale cached result from another backend or configuration.
- **Atomic publication:** generated artifacts, host tool outputs, stale-file removals and the output manifest are staged beside the output directory and committed in one transaction, so a failed generation leaves earlier published output intact.

## Authoring a third-party backend package

Any language that targets the WebAssembly Component Model can implement a Flick backend by targeting the `flick:backend@0.1.0` world (`flick-backend/wit/backend.wit`):

```wit
package flick:backend@0.1.0;

world backend {
    import inputs;
    export describe: func() -> descriptor;
    export check-target: func(request: build-request) -> result<target-support, list<diagnostic>>;
    export generate: func(request: build-request) -> result<build-result, list<diagnostic>>;
    export package-files: func() -> list<string>;
    export packages: func(
        files: list<project-file>,
        dependencies: list<dependency>,
        options: list<option-value>,
    ) -> packages;
    export flick:command/command@0.1.0;
}
```

### Key contract guarantees

- **Sandboxed pure guest:** `inputs.read` is the only import. WASI filesystem, clock, randomness, network and process APIs are denied and trap if called. Every byte the guest reads must come from the content-addressed request manifest.
- **Explicit handshake & preflight:** `describe` declares exact `[minimum, maximum_exclusive)` ranges for `backend-api`, `ir`, `language` and `runtime-abis`, supported environments/profiles/modes, options, `launch-runtimes`, `host-tools`, foreign metadata and optional `value-abi`. Unsupported IR constructs are refused with source spans in `check-target` (`FLICK-P012`) before generation runs.
- **Host tool grants (`[tools]`):** a backend never spawns processes directly. Instead, `BuildResult.steps` declares a DAG of `toolchain-step` records referencing pinned host tools (`node`, `go`, `pnpm`). The project must explicitly grant every tool in `[tools]`:

```toml
[tools]
"example/backend" = ["go"]
```

  An ungranted tool fails with `FLICK-P051` before any step executes. The host hashes the tool executable on `PATH`, rechecks its digest before every step, runs each step in a fresh isolated directory beside the output with a cleared environment, and reads back only declared regular-file outputs (`FLICK-P050`).
- **Launch pins:** `BuildResult.launches` names runnable and test artifacts by artifact ID, optionally paired with a pinned `launch.runtime` (such as `node`) or `none` for a directly executable native binary. Launch runtimes declared in `descriptor.launch-runtimes` do not need a `[tools]` entry, but their executable version, SHA-256 digest and runtime dependency pins are rechecked before every launch.

## Migration examples

### 1. Migrating legacy `[build]` and `[js]` settings to `[target.js]`

Legacy keys under `[build]` (`emit`, `emit-dts`, `effect`, `promise-facade`, `schema-aot`, `ui`) and `[js]` / `[js.dependencies]` fail with `FLICK-P057`. Replace them with `[build] target` and `[target.js]`:

Before:

```toml
[build]
emit = "ts"
emit-dts = true
effect = "effect"
promise-facade = true

[js.dependencies]
left-pad = "^1.3.0"
```

After:

```toml
[build]
target = "js"

[target.js]
typescript = true
declarations = true
runtime = "effect"
promise-facade = true

[target.js.dependencies]
left-pad = "^1.3.0"
```

### 2. Switching between `rt` and `effect`

To move an existing Effect project onto Flick's default `rt` runtime (or verify compatibility before changing `flick.toml`), first test a single invocation with `--runtime rt`:

```sh
flick check --runtime rt
flick test --runtime rt
```

Once preflight reports no reachable Effect-adapter-only bindings (`FLICK-P064`), update `flick.toml` and keep `@flick-lang/runtime` in `package.json`:

```toml
[target.js]
runtime = "rt"
```

To opt into the Effect adapter for a project that uses Effect-backed modules such as `Stream`, set `runtime = "effect"` and add `@flick-lang/effect-ts` to `package.json`:

```toml
[target.js]
runtime = "effect"
```

### 3. Pinning an external or preview backend

To pin a local or third-party backend component (for example the preview `flick/lua` component or a custom backend that runs a host tool):

```toml
schema-version = 1

[project]
kind = "app"
entry = "App.Main"

[build]
environment = "lua/love"

[build.backend]
id = "flick/lua"
version = "0.1.0"
sha256 = "<sha256 of backends/lua/package/component.wasm>"
component = "../backends/lua/package/component.wasm"

[target.lua]
dialect = "luajit"
```

If the custom backend declares build-time `ToolchainStep` records (for instance invoking `go` or `node`), grant the exact tool under `[tools]`:

```toml
[tools]
"example/backend" = ["go"]
```
