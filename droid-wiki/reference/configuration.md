# Configuration

Active contributors: bondiano, Vassiliy — non-bot author names on `origin/main`; names may be aliases.

Flick has four distinct configuration layers: a project's `flick.toml`, invocation flags, compiler Cargo capabilities, and host environment. Backend defaults differ between native projects, standalone files, and the browser API.

This page follows the inspected working tree. `flick-core/Cargo.toml` has pre-existing uncommitted changes; its capability table below is not a statement about an already published compiler.

## Project manifests

`flick-core/src/project/config/parse.rs` requires `schema-version = 1` and rejects unknown keys. A manifest needs `[project]`, `[workspace]`, or both. Workspace-only roots cannot carry project `[build]`, `[sql]`, `[lint]`, dependency, package, or per-project plugin tables.

| Table | Contract | Source |
| - | - | - |
| `[project]` | Required `kind = "app"` or `"lib"`; source/test directories, `entry` or named `entries`, optional runner | `flick-core/src/project/config/parse.rs` |
| `[build]` | Output/profile/target, runtime backend, declarations, solver limits, UI module | `flick-core/src/project/config.rs` |
| `[package]` | Published name/version, compiler requirement, optional plugin component, file loaders, hooks, watched names | `flick-core/src/project/config/deps.rs` |
| `[dependencies]` | Source libraries: path, tagged Git source with optional subdirectory, or SHA-256-pinned URL archive | `flick-core/src/project/config/deps.rs` |
| `[js.dependencies]` | JS runtime requirements collected across the source-library closure | `flick-core/src/project/js.rs` |
| `[sql]` | Dialect selection; only `postgresql` is currently accepted as supported | `flick-core/src/project/config/parse.rs` |
| `[lint]` | Allowed codes, `no-let-assert`, positive structural-warning thresholds | `flick-core/src/project/config/parse.rs` |
| `[plugins.<package>]` | `fs-read`, `net`, `env` grants and disabled block heads/classes; no grants by default | `flick-core/src/project/config.rs`, `flick-core/src/project/config/deps.rs` |
| `[workspace]`, `[[workspace.plugins]]` | Members and plugin dependencies/grants inherited by members | `flick-core/src/project/config/workspace.rs`, `flick-core/src/project/inherit.rs` |
| `[scripts]` | Shell step or ordered steps; `@name` refers to another script | `flick-core/src/project/config/parse.rs`, `flick/src/commands/script.rs` |

Scripts are trusted shell execution, not sandboxed language expressions. Plugin grants are a separate boundary; see [plugin hosting](../systems/project-and-cli/plugin-host.md).

### Actual project defaults

| Setting | Default | Source |
| - | - | - |
| `project.source-dir`, `project.test-dir` | `src`, `test` | `flick-core/src/project/config/parse.rs` |
| `project.kind`, app entry module | No default; app needs `entry` or nonempty `entries` | `flick-core/src/project/config/parse.rs` |
| Single `entry` name | `main` | `flick-core/src/project/config.rs` |
| Entry target / runner | Server target; no custom runner unless inherited from the project's runner | `flick-core/src/project/config/parse.rs` |
| `build.out-dir` | `dist`, relative to project root | `flick-core/src/project/config.rs`, `flick/src/commands/build.rs` |
| `build.profile`, `build.emit` | `debug`, `js` | `flick-core/src/project/config.rs` |
| `build.effect` | `rt` — Flick's owned runtime | `flick-core/src/project/config.rs` |
| `build.emit-dts`, `build.promise-facade`, `build.schema-aot` | All `false` | `flick-core/src/project/config.rs` |
| `build.solver-steps`, `build.solver-depth` | `100000`, `256`, per definition | `flick-core/src/types/mod.rs:144-159` |
| `build.labels`, `build.ui` | `all`, no custom UI module | `flick-core/src/codegen/mod.rs`, `flick-core/src/project/config.rs` |
| `sql.dialect` | `postgresql` | `flick-core/src/project/config/parse.rs` |
| `lint.allow`, `lint.no-let-assert` | Empty list, `false` | `flick-core/src/project/config/parse.rs` |
| `lint.max-function-lines`, `lint.max-nesting` | `60`, `4` | `flick-core/src/types/mod.rs:161-179` |
| `lint.max-complexity`, `lint.max-params` | `10`, `5` | `flick-core/src/types/mod.rs:161-179` |

The default runner still validates the type/effect contract of `main`; a server target is not an automatic provision of arbitrary services. See [entry validation](../systems/project-and-cli/project-loading-and-packages.md).

## CLI selection and precedence

`--project <directory-or-toml>` selects explicitly; otherwise discovery searches upward for the nearest `flick.toml`. Workspace `--package`/`-p` conflicts with `--project`. At a workspace-only root, eligible whole-project commands fan out to members unless a member is selected; explicit files and long-running modes have separate routing rules.

| Invocation setting | Default / precedence | Source |
| - | - | - |
| `--diagnostics`, `--color` | `human`, `auto` | `flick/src/args.rs` |
| `--quiet`, `--verbose`, `--warnings-as-errors`, `--offline` | All absent/false; quiet conflicts with verbose | `flick/src/args.rs` |
| `build --out-dir` | Flag relative to invocation directory; otherwise project setting; otherwise invocation-directory `dist` | `flick/src/commands/build.rs:275-305` |
| `build --emit`, `--profile`, `--labels` | Supplied value wins over project value; file-only defaults are JS/debug/all | `flick/src/commands/build.rs:275-344` |
| `build --emit-dts`, `--promise-facade`, `--schema-aot` | Enable by OR with the corresponding project boolean; no negative override here | `flick/src/commands/build.rs:299-310` |
| `--rt` | Explicitly selects owned runtime; project `effect = "rt"` selects it for non-Go output | `flick/src/commands/build.rs:299-305` |
| `build --emit-ir` | Absent by default; a bare flag means JSON; explicit `json` or `cbor` | `flick/src/args.rs` |
| `test --jobs`, `--reporter`, `--seed` | `1`, `human`, no CLI-provided seed | `flick/src/args.rs` |
| `new/init --template` | `app`; alternatives `cli`, `fullstack`, `lib`, `plugin` | `flick/src/args/tools.rs` |
| `run/dev --devtools` | Disabled unless supplied; bare flag uses `ws://localhost:34437` | `flick/src/args/tools.rs` |
| `install --locked` | False unless supplied; enforces Flick pins and npm frozen-lock behavior | `flick/src/args/tools.rs`, `flick/src/commands/deps.rs` |

### Backend caveats

- Native **project** defaults are RT. A file-only build without project configuration falls back to Effect, in `flick/src/commands/build.rs:507-511`.
- `flick-wasm/src/lib.rs` explicitly emits with `Backend::Effect`; it does not inherit the native project default.
- `effect = "none"` is a restricted pure-code build, not an asynchronous runtime without dependencies.
- Go emission uses its own runtime. Explicit `--rt` with Go or Promise façades is rejected, in `flick/src/commands/build.rs:115-123`; a project's default RT does not by itself prohibit `--emit go`.
- RT bridging depends on adapter availability and per-binding routing; see [runtimes](../packages/runtimes/index.md).

## Cargo capabilities

`flick-core/Cargo.toml` enables these **eight default features**. In-memory configuration types, Host-backed loading/checking, and primary JSON diagnostics remain available without them.

| Core feature | Capability / dependency gate |
| - | - |
| `ide` | Retained editor metadata and queries; also enables `typescript` and `diagnostic-fixes` |
| `build-artifacts` | Published interfaces, package artifacts, publication fingerprints |
| `source-maps` | Source markers/mappings and emitted map files |
| `typescript` | TS output, declarations, Promise façades |
| `diagnostic-fixes` | Lazy fixes/edits and their JSON fields; not the primary diagnostics |
| `native-project` | Native discovery/TOML/packages/cache contracts; activates optional `toml`, `semver`, `sha2` |
| `terminal-diagnostics` | Human rendering; activates optional `annotate-snippets` |
| `builtin-docs` | Documentation in embedded builtins; stripping is lexical, not string deletion |

Additional feature relationships:

| Manifest | Feature selection |
| - | - |
| `flick-core/Cargo.toml` | Nondefault `embedded-std` embeds the implicit standard library for package-less hosts |
| `flick/Cargo.toml` | Uses core defaults plus `ide`; nondefault `wasm-plugins` activates exact `wasmtime = 48.0.8` |
| `flick-wasm/Cargo.toml` | Disables core defaults; requests `embedded-std` + `ide` (therefore TS and fixes) |
| `flick-model/Cargo.toml` | Disables core defaults; requests `embedded-std` |
| `flick-ir/Cargo.toml` | Nondefault `json` and `cbor` activate their serializers |
| `flick-plugin/Cargo.toml` | Nondefault `guest` activates `wit-bindgen` component glue |
| `tests/plugins/pairs/Cargo.toml` | `guest` forwards to the SDK's guest feature |
| `packages/effect-ts/tools/surface/Cargo.toml` | Defaults to `ide`, forwarded to core's `ide` feature; this crate is uncommitted in the snapshot |

The inspected `flick-wasm/Cargo.toml` has **no crate-local `server-builtins` feature**. Older reduced-surface setup prose is not the current manifest contract. Cargo unifies dependency features: validate minimal consumers separately rather than infer their size/capabilities from a workspace build. See [support crates](../packages/support-crates.md).

Rust release profile (`Cargo.toml`) uses thin LTO, one codegen unit, stripping. The custom `e2e` profile inherits release but disables LTO/stripping, uses 16 codegen units, and enables debug assertions, overflow checks, and incremental compilation. These Cargo profiles are different from a Flick program's `build.profile`.

## Host environment

This table covers compiler/tool configuration, not every environment variable an application can read.

| Variable | Behavior when set / unset | Source |
| - | - | - |
| `FLICK_STD_PATH` | Nonempty override for first implicit package (`std`); otherwise project/checkout/installed-package preparation supplies sources | `flick/src/stdlib.rs`, `flick-core/src/types/builtins.rs`, `flick-core/src/implicit_packages.rs` |
| `FLICK_STORE` | Nonempty source-package store override; otherwise `<cache-dir>/pkg` | `flick-core/src/project/store.rs` |
| `XDG_CACHE_HOME`, `LOCALAPPDATA`, `HOME` | Cache precedence: nonempty XDG, then LOCALAPPDATA, then HOME/.cache; append `flick` | `flick-core/src/project/store.rs` |
| `CI` | Presence enables plugin snapshot/offline mode, even without `--offline`; also suppresses progress animation | `flick/src/main.rs`, `flick/src/progress.rs` |
| `FLICK_DTS_IMPORTER` | Importer script override; otherwise `<project>/node_modules/@flick-lang/dts-importer/index.mjs` | `flick/src/commands/ffi.rs:42-47` |
| `FLICK_PLUGIN_TRACE` | Append plugin-call trace lines to the named file; unset means no trace wrapper | `flick/src/plugins/mod.rs:1-39` |
| `RUST_LOG` | LSP tracing filter through tracing-subscriber's default environment filter; missing/invalid falls back to `info` | `flick/src/lsp/logging.rs` |
| `NO_COLOR` | Presence disables automatic diagnostic color; explicit color choice is separate | `flick/src/report.rs` |
| `TERM`, `COLUMNS` | Terminal/progress presentation; `TERM=dumb` disables interactive progress | `flick/src/progress.rs` |
| `GOOS`, `GOARCH` | Go toolchain target inputs; `GOOS` also controls executable suffix | `flick-core/src/build/go.rs`, `flick/src/cache.rs` |
| `CGO_ENABLED` | Preserved when supplied; compiler sets `0` for Go builds when absent | `flick/src/toolchain.rs:82-101` |
| `FLICK_PG_TEST_URL` | Optional live PostgreSQL driver-test connection; absent tests can skip live DB work | `runtime-go/flickpg/pg_test.go` |

Go build subprocesses set `GOFLAGS=-mod=mod` and `GOTOOLCHAIN=local`, in `flick/src/toolchain.rs`. `--offline` is not a universal network sandbox: Go may need its module cache, project scripts execute shell commands, and standalone implicit-package preparation has its own fetch options.

## Key source files and change guidance

| Source | Change here when… |
| - | - |
| `flick-core/src/project/config.rs`, `flick-core/src/project/config/parse.rs` | Adding settings or changing defaults/schema |
| `flick/src/args.rs`, `flick/src/args/tools.rs`, `flick/src/commands/build.rs` | Adding flags or changing precedence |
| `flick-core/Cargo.toml`, `flick-wasm/Cargo.toml` | Changing capability boundaries |
| `flick-core/src/project/store.rs`, `flick/src/stdlib.rs`, `flick/src/toolchain.rs` | Changing host/toolchain selection |
| `flick-core/src/project/config_tests.rs`, `flick/tests/cli_toolchain.rs` | Pinning configuration regressions |

Behavior changes need the applicable project/CLI tests and `just e2e-tests`; capability changes need `just feature-check`, `just feature-test`, and separate WASM builds. These are guidance, not checks run for this prose. Continue with [dependency requirements](dependencies.md) or [command lifecycles](../systems/project-and-cli/commands-and-toolchain.md).
