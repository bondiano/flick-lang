# Contributor tooling

Active contributors: bondiano, Vassiliy Kuzenkov, chaoseternally — nonbot author names recorded on `origin/main` for `justfile`, tool/editor sources, and environment configuration.

`justfile` coordinates the compiler, JS packages, runtime targets, browser compiler, editors, and measurement harnesses. Read its actual dependencies and package scripts before running a generator or a broad suite; `CONTRIBUTING.md` documents setup.

## Pinned and optional tools

| Tool | Source of truth | Role |
| - | - | - |
| Rust 1.95.0 | `rust-toolchain.toml` | rustc/Cargo with rustfmt, Clippy, `wasm32-unknown-unknown`, `wasm32-wasip2` |
| Node.js ≥24.18.0 | `package.json`, `flick/src/toolchain.rs` | Generated JS/TS, runtime/site/tool tests, dependency helpers |
| pnpm 12.16.1 | `package.json` `packageManager` | Frozen workspace installation; project-specific pins govern dependency writes |
| `just` | `justfile`, `CONTRIBUTING.md` | Repository task composition; CI pins 1.59.0 |
| `cargo-machete` | `CONTRIBUTING.md`, `.github/workflows/ci.yml` | Unused Rust dependency check; pinned 0.9.2 |
| `cargo-nextest` | `.config/nextest.toml`, `.github/workflows/ci.yml` | Ignored integration execution, process isolation and JUnit; CI pins 0.9.148 |
| Go ≥1.23 for target work | `CONTRIBUTING.md`, `runtime-go/go.mod` | Native Go output and runtime tests; CI reads the module toolchain requirement |
| Lean/lake | `formal/lean-toolchain` | Formal development, separately from Rust/Cargo |
| Chromium, Docker, hurl | `justfile` and relevant test headers | Browser, PostgreSQL, and Conduit integration, not every compiler edit |
| wasm-bindgen CLI 0.2.130 | `justfile` `docs-wasm` | Browser bindings; installed under `target/tools` by the recipe |

`flake.nix` provides `nix develop` with the pinned Rust toolchain, rust-analyzer/rust-src, Node, pnpm, Go, just, machete, Nextest, and jq. The shell is a tool environment, not a substitute for all browser/database services. Package builds and platform limits are covered in [deployment](../deployment.md#nix-package-and-development-shell).

## Everyday commands

```sh
pnpm install --frozen-lockfile
just fmt
just lint
just test
just check
```

`just fmt` formats Rust; it is not the Flick language formatter. `flick fmt` formats Flick programs, while `just docs-fix` builds the CLI and applies safe fixes to site Markdown blocks. Both language fix/format workflows can rewrite files: inspect the selected paths and review the diff.

`just check = lint + test`. `test = unit + feature-test`, and `unit` excludes `flick-model`. Use [testing](testing.md) for the full recipe map rather than interpreting the “Everything CI runs” comment above `check` as a complete CI dependency graph.

`justfile` exports `FLICK_STD_PATH` for every recipe. Direct commands need an explicit local std environment when testing this checkout. Native CLI development generally uses `cargo build -p flick`; end-to-end recipes choose `--profile e2e` and pass that compiler to dependent packages.

## Compiler capabilities and browser build

`flick-core` defaults to full capabilities; `flick-wasm` opts out and enables only its required surface. Workspace Cargo feature unification can mask minimal-consumer mistakes.

- `just feature-check`: strict Clippy on minimal and individual capability combinations.
- `just feature-test`: corresponding behavior tests, including the embedded standard library.
- `just wasm`: isolated browser-compiler build with size-oriented release settings and a raw-byte limit.
- `just docs-wasm`: WASM build plus generated browser bindings.
- `just wasm-plugins`: a **different** WASM route, building the test plugin as a component for Wasmtime.

`wasm32-unknown-unknown` browser modules and `wasm32-wasip2` plugin/editor components have different hosts. Do not substitute one target for the other.

## Site and editor tools

`just docs-dev` builds the browser compiler and native CLI, then starts Astro. `just docs` creates `docs/dist`; production preview is `pnpm -C docs preview`. Follow the runtime/adapter preparation in `CONTRIBUTING.md` when starting from a clean dependency tree.

| Tool or source | Contributor workflow |
| - | - |
| `docs/scripts/parity.mjs` | Check locale pairs and original-source hashes; stamp a translation only after reviewing it against the original |
| `docs/scripts/links.mjs` | Check built site navigation/links after `just docs` |
| `docs/scripts/chunks.mjs` | Check built JS chunk budgets |
| `tools/astro-flick/` | Vite/Astro integration invokes the native compiler and watches its input inventory |
| `tools/content-mapper/` | npm launcher delegates TypeScript projection protocol to the CLI |
| `tools/dts-importer/` | Compiled Flick program plus TypeScript primitives supplies declaration surfaces to `flick ffi` |
| `editors/vscode/` | Language-client extension, TextMate grammar, VSIX packaging checks; `flick.path` chooses the server |
| `editors/tree-sitter-flick/` | `just tree-sitter` regenerates the lexical grammar and runs its corpus |
| `editors/zed/` | `just zed` builds the component; CI separately runs the extension crate's locked tests |

Both desktop editors use `flick lsp --stdio`. Rust editor aids do not replace the Flick server. The language server and Content Mapper share compiler IDE/projection facilities; see [editor protocols](../systems/project-and-cli/language-server-and-ide.md).

The public site's English/Russian pairs are under `docs/src/content/docs/`. English is original except `spec/`, whose original is Russian. Interactive widgets/playground behavior belongs in Flick; TypeScript is for mounting and browser/compiler/worker interop. See [browser tools](../systems/browser-tools.md).

## Measurement and generated data

| Command/family | Evidence produced and cautions |
| - | - |
| `just perf` | `target/perf.json` and failures against `tools/perf/budgets.json`; compiler, LSP, server/load, and Go size/resource metrics |
| `just bundle-size` | Release program/site gzip and retained-module checks against `tools/bundle-size/programs.mjs`; raw WASM has a separate gate |
| `just ratchet` | Compares performance/size limits with `origin/main` by default; loosening needs a reviewed adjacent `reason` |
| `just bench`, `just lsp-bench`, `just lsp-soak` | Focused whole-process timing, editor latency, and long-session memory evidence |
| `just load-json`, `just load-ssr`, `just load-ssr-browser` | Build/start a test server and validate request-specific results under load; read ports, process lifetimes, CPU-affinity and browser requirements first |
| `just stress`, `just conformance`, `just conformance-rt` | Seeded scheduling and trace/runtime-law checks, not exhaustive schedule or implementation proofs |
| `tools/go-*/` | Upstream Node/ICU/Effect-derived compatibility tables and corpora; review provenance and matching Go tests before regenerating |
| `just effect-bump <version>`, `just effect-coverage` | Upgrade the pinned Effect surface intentionally; coverage classification must still be reviewed |
| `tools/rt-exports.mjs` | Generate/check runtime export/stub manifests consumed by compiler bindings; inspected output ownership includes uncommitted source changes |
| `just formal-schemes` | Regenerate Lean Prelude schemes from checker data; review diff, then model and Lean gates |
| `just tokens` | Count application source; optional provider credentials can transmit source to external token-counting services |

Do not overlap timing/performance measurements with builds or unrelated suites. Linux RSS and `taskset` recipes are not portable measurements on macOS. Report host, client, input, warm-up/repetition, actual result, and threshold separately. A sampled generator oracle or budget does not establish all-input correctness.

Do not run upgrade tools, networked generators, or credentialed token APIs as routine wiki validation. No such actions were performed for these pages.

The detailed consumers, generator families, and methodology are in [developer tools](../systems/developer-tools.md).

## CI and automation

`.github/workflows/ci.yml` distributes recipes into separate jobs; nightly adds million-input fuzzing and performance, and release reuses CI. The setup Action at `action.yml` installs a released compiler, not the current source tree.

`.github/workflows/droid-wiki-refresh.yml` was **untracked user work** in the inspected snapshot. Its read/generate and write/publish jobs describe working-tree-only automation, not a verified workflow on the default branch. This wiki task did not modify it. See [distribution and automation](../deployment.md).

## Key source files

| Source | Responsibility |
| - | - |
| `justfile`, `CONTRIBUTING.md` | Task composition and setup |
| `rust-toolchain.toml`, `package.json`, `pnpm-workspace.yaml` | Toolchain and workspace pins |
| `flake.nix`, `nix/package.nix` | Development shell and packaged compiler |
| `.config/nextest.toml`, `.github/workflows/ci.yml` | Integration execution/reporting |
| `tools/perf/index.mjs`, `tools/perf/ratchet.mjs` | Measurement/report semantics and budget changes |
| `tools/bundle-size/programs.mjs`, `tools/rt-exports.mjs` | Size policy and generated binding surfaces |

Descriptions reflect the dirty `main` working tree based at `6e9096fb`. This page did not run builds, generators, benchmarks, or executable suites; start with the [development workflow](development-workflow.md) for an actual change.
