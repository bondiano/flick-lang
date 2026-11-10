# Dependencies

Active contributors: bondiano, Vassiliy — non-bot author names on `origin/main`; names may be aliases.

Flick's dependencies fall into separate compiler, JavaScript tooling/runtime, source-library, and optional Go-integration channels. The counts below measure direct manifest declarations, not an installed transitive tree or a vulnerability/freshness assessment.

## Counting method

Measurements parse the **2026-11-10 working-tree manifests** with Python `tomllib`/`json`:

- Rust scope: all **15 members** listed in `Cargo.toml`, plus the separately built Zed crate. Count dependency keys in normal, dev, and build tables, including target-specific tables. Optional declarations count whether or not enabled.
- A declaration in two categories counts in each category. **Distinct** is the union of dependency names within that package, not the sum of those columns and not a global unique-package count.
- **Local normal** counts normal Rust declarations with `path`; it is a subset of normal, not an extra category. Target-specific normal entries are included even on another host; `rustix` is Unix-only.
- npm scope: root plus the **14 members** explicitly listed in `pnpm-workspace.yaml`; standalone authored examples are shown separately. Count `dependencies`, `devDependencies`, and `peerDependencies`; all measured manifests have zero `optionalDependencies`. Optional peers still count as peers and may also be dev dependencies.
- Exclude test/FFI fixture package manifests, generated artifacts, lockfile transitives, and dependency-installed manifests. No package-manager install or network lookup was used.

`Cargo.toml`, `flick-core/Cargo.toml`, and `Cargo.lock` have pre-existing uncommitted changes; the surface-test crate under `packages/effect-ts/tools/surface/` is uncommitted. These are current-checkout counts, not counts of committed `origin/main`.

## Rust workspace

| Package / manifest | Normal | Dev | Build | Distinct | Local normal |
| - | -: | -: | -: | -: | -: |
| `flick-core` — `flick-core/Cargo.toml` | 16 | 4 | 3 | 18 | 8 |
| `flick-ir` — `flick-ir/Cargo.toml` | 3 | 0 | 0 | 3 | 0 |
| `flick` — `flick/Cargo.toml` | 15 | 4 | 0 | 18 | 3 |
| `flick-pretty` — `flick-pretty/Cargo.toml` | 2 | 0 | 0 | 2 | 0 |
| `flick-wasm` — `flick-wasm/Cargo.toml` | 3 | 0 | 0 | 3 | 1 |
| `flick-model` — `flick-model/Cargo.toml` | 2 | 1 | 0 | 3 | 1 |
| `flick-plugin` — `flick-plugin/Cargo.toml` | 3 | 0 | 0 | 3 | 1 |
| `flick-plugin-macros` — `flick-plugin/macros/Cargo.toml` | 2 | 0 | 0 | 2 | 0 |
| `flick-plugin-cli` — `plugins/cli/Cargo.toml` | 1 | 2 | 0 | 3 | 1 |
| `flick-plugin-derive` — `plugins/derive/Cargo.toml` | 1 | 2 | 0 | 3 | 1 |
| `flick-plugin-refined` — `plugins/refined/Cargo.toml` | 1 | 0 | 0 | 1 | 1 |
| `flick-plugin-sql` — `plugins/sql/Cargo.toml` | 3 | 2 | 0 | 5 | 1 |
| `flick-plugin-ui` — `plugins/ui/Cargo.toml` | 1 | 0 | 0 | 1 | 1 |
| `flick-test-pairs` — `tests/plugins/pairs/Cargo.toml` | 2 | 0 | 0 | 2 | 2 |
| `flick-effect-surface` — `packages/effect-ts/tools/surface/Cargo.toml` | 0 | 4 | 0 | 4 | 0 |
| **Workspace column totals** | **55** | **19** | **3** | **71** | **21** |
| Separate `zed_flick` — `editors/zed/Cargo.toml` | 1 | 0 | 0 | 1 | 0 |

The workspace therefore declares **77 category entries**; the sum of per-package name unions is 71. Neither is a resolved dependency count. For example, core's `flick-ir` appears in normal/dev/build tables; `toml` in normal/dev; `serde` and `serde_json` in normal/build. CLI's `serde_json` appears in normal/dev.

### Feature and platform gates

| Package | Optional/target dependency | Activation |
| - | - | - |
| `flick-core` | `toml`, `semver`, `sha2` | Default feature `native-project` |
| `flick-core` | `annotate-snippets` | Default feature `terminal-diagnostics` |
| `flick-ir` | `serde_json`, `ciborium` | Nondefault `json`, `cbor`; JSON also enables float round-trip support |
| `flick` | `wasmtime = "=48.0.8"` | Nondefault `wasm-plugins`; runtime, Cranelift, component-model, std only, with Wasmtime defaults disabled |
| `flick` | `rustix` | `cfg(unix)` normal dependency, counted above |
| `flick-plugin` | `wit-bindgen = "0.63.0"` | Nondefault `guest` |

Rust version strings without `=` are Cargo requirements, not necessarily exact pins; `Cargo.lock` supplies the resolved workspace versions. Native CLI/core default features are broader than the browser and model selections. See the [capability reference](configuration.md#cargo-capabilities).

### Why the main external crates exist

| Boundary | Direct external families | Owning manifests |
| - | - | - |
| Serialization / persistence | `serde`, `serde_json`, `ciborium`, `toml` | `flick-core/Cargo.toml`, `flick-ir/Cargo.toml`, `plugins/sql/Cargo.toml` |
| Diagnostics / layout | `annotate-snippets`, `strsim`, `typed-arena`, `unicode-segmentation` | `flick-core/Cargo.toml`, `flick-pretty/Cargo.toml` |
| Native CLI / processes | `clap`, `anstyle`, `signal-hook`, Unix `rustix` | `flick/Cargo.toml` |
| Fetching / package integrity | `ureq`, `flate2`, `tar`, `sha2`, `semver` | `flick/Cargo.toml`, `flick-core/Cargo.toml` |
| Tracing | `tracing`, `tracing-subscriber` | `flick/Cargo.toml`, `flick-core/Cargo.toml` |
| WASM boundary | `wasm-bindgen`, optional `wit-bindgen`, optional `wasmtime` | `flick-wasm/Cargo.toml`, `flick-plugin/Cargo.toml`, `flick/Cargo.toml` |
| Macro generation | `proc-macro2`, `quote` | `flick-plugin/macros/Cargo.toml` |
| Development tests | `insta`, `proptest` | `flick-core/Cargo.toml`, `flick/Cargo.toml`, `flick-model/Cargo.toml`, first-party plugin manifests |

## JavaScript workspace

“Prod” is the manifest's `dependencies` table, even for a private benchmark/docs tool. “Distinct” unions prod/dev/peer names within that row.

| Package / manifest | Prod | Dev | Peer | Distinct |
| - | -: | -: | -: | -: |
| Root — `package.json` | 0 | 5 | 0 | 5 |
| `@flick-lang/runtime` — `runtime/package.json` | 0 | 13 | 2 | 13 |
| `@flick-lang/effect-ts` — `packages/effect-ts/package.json` | 4 | 13 | 3 | 17 |
| `@flick-lang/dts-importer` — `tools/dts-importer/package.json` | 3 | 4 | 0 | 7 |
| `@flick-lang/content-mapper` — `tools/content-mapper/package.json` | 0 | 1 | 0 | 1 |
| `@flick-lang/astro` — `tools/astro-flick/package.json` | 0 | 0 | 0 | 0 |
| `@flick-lang/bundle-size` — `tools/bundle-size/package.json` | 5 | 0 | 0 | 5 |
| `@flick-lang/load` — `tools/load/package.json` | 6 | 0 | 0 | 6 |
| `@flick-lang/load-ssr` — `tools/load/ssr/package.json` | 2 | 0 | 0 | 2 |
| `@flick-lang/tokens` — `tools/tokens/package.json` | 1 | 0 | 0 | 1 |
| `@flick-lang/docs` — `docs/package.json` | 13 | 7 | 0 | 20 |
| VS Code `flick` — `editors/vscode/package.json` | 1 | 0 | 0 | 1 |
| `@conduit/contract` — `examples/conduit/contract/package.json` | 2 | 0 | 0 | 2 |
| `@conduit/server` — `examples/conduit/server/package.json` | 2 | 0 | 0 | 2 |
| `@conduit/client` — `examples/conduit/client/package.json` | 4 | 3 | 0 | 7 |
| **Root + workspace column totals** | **43** | **46** | **5** | **89** |

There are **94 category entries** in this scope. React/React DOM occur as both development dependencies and optional peers; adapter `pg` does too. That explains the five-entry difference from the summed per-package unions.

Authored examples outside the pnpm member list:

| Manifest | Prod | Dev | Peer | Distinct |
| - | -: | -: | -: | -: |
| `examples/first-slice/package.json` | 2 | 0 | 0 | 2 |
| `examples/mtl/package.json` | 2 | 0 | 0 | 2 |
| `examples/observability/package.json` | 2 | 0 | 0 | 2 |
| `examples/cli-tool/package.json` | 2 | 0 | 0 | 2 |
| `examples/typed-interpreter/package.json` | 2 | 0 | 0 | 2 |

These add ten category entries; including them gives **53 prod, 46 dev, 5 peer**, or **104 entries** and **99 summed per-package distinct names**. Fixtures remain excluded.

## Runtime and toolchain requirements

| Surface | Recorded requirement | Source |
| - | - | - |
| Owned runtime | No `dependencies`; optional React/React DOM peers `^19.0.0` | `runtime/package.json` |
| Effect adapter | Exactly `effect`, `@effect/platform-node`, `@effect/platform-browser` **4.1.2**, plus `@flick-lang/runtime = workspace:*` | `packages/effect-ts/package.json` |
| Adapter optional peers | `pg ^8.23.0`, `react ^19.0.0`, `react-dom ^19.0.0` | `packages/effect-ts/package.json` |
| Effect node shared package | Workspace override **4.1.2** | `pnpm-workspace.yaml` |
| Tested UI development packages | React/React DOM **19.3.1**, pg **8.23.1** | `runtime/package.json`, `packages/effect-ts/package.json` |
| CLI runtime/adapter compatibility | Runtime **0.1.0**, adapter **0.1.0**, Effect **4.1.2** | `flick/src/toolchain.rs:13-23` |
| JS development toolchain | Node **>=24.18.0**, pnpm **12.16.1**, TypeScript **7.1.0-beta** | `package.json`, `flick/src/toolchain.rs` |
| Rust / Lean | Rust **1.95.0**; Lean **4.29.1** | `rust-toolchain.toml`, `formal/lean-toolchain` |

The owned runtime's optional UI peers do not make Effect a mandatory dependency. Adapter bridges and native export tables select individual capabilities; see [execution runtimes](../packages/runtimes/index.md). The CLI's installed-version check rejects mismatches for packages it finds; it skips missing packages rather than certify a complete installation, in `flick/src/toolchain.rs:58-79`.

The manifests above record requirements. No registry comparison, update-age judgment, security advisory lookup, or installed-tree audit was performed.

## Go optional integrations

`runtime-go/go.mod` declares **3 direct** and **6 indirect** module requirements for development of all runtime packages. Its `go` directive is **1.23.0**.

| Direct module | Version | Why optional in generated applications |
| - | - | - |
| `github.com/jackc/pgx/v5` | `v5.7.6` | `runtime-go/flickpg/` is selected for the reached PostgreSQL layer binding |
| `github.com/andybalholm/brotli` | `v1.2.7` | `runtime-go/flickcompress/` is selected for reached compression middleware |
| `github.com/klauspost/compress` | `v1.18.4` | Same compression package, for zstd |

PostgreSQL's `runtime-go/flickpg/modules.txt` lists a seven-module closure: pgx plus `pgpassfile v1.0.0`, `pgservicefile v0.0.0-20240606120523-5a60cdf6a761`, `puddle/v2 v2.2.3`, `x/crypto v0.41.0`, `x/sync v0.16.0`, and `x/text v0.28.0`. Compression's `runtime-go/flickcompress/modules.txt` lists the two direct compression modules above.

`flick-core/src/codegen/go/main_file.rs` selects optional packages by reachable bindings. `flick-core/src/build/go.rs` copies runtime sources and filters generated module requirements by those `modules.txt` lists, with pinned sum evidence. Without a reached optional package, the generated module has no external requirements. The development Go module's nine requirements are therefore **not** every generated program's dependency footprint.

Go may download required modules when absent from its cache; the CLI's `--offline` is not a Go network sandbox. See [Go runtime](../packages/runtimes/go-runtime.md).

## Flick source libraries

`packages/core/flick.toml` and `packages/std/flick.toml` each declare **zero `[dependencies]` and zero `[js.dependencies]` entries**. Both are source libraries at 0.1.0 with compiler requirement `>=0.1, <0.2`. This does not mean every standard operation runs without a runtime: foreign bindings and backend routing are a separate contract, and `std` is supplied implicitly through `flick-core/src/implicit_packages.rs`.

Project source dependencies use `flick.lock`; npm uses `pnpm-lock.yaml`; Rust uses `Cargo.lock`; Go uses `runtime-go/go.sum`. The channels are not interchangeable. See [project packages](../systems/project-and-cli/project-loading-and-packages.md).

## Key source files and update obligations

| Source | Review when changing dependencies |
| - | - |
| `Cargo.toml`, crate `Cargo.toml` manifests, `Cargo.lock` | Membership, capability isolation, MSRV, resolved versions |
| `pnpm-workspace.yaml`, `package.json`, `pnpm-lock.yaml` | Membership, package-manager pin, Effect override |
| `runtime/package.json`, `packages/effect-ts/package.json`, `flick/src/toolchain.rs` | Runtime ABI versions, optional peers, exact adapter compatibility |
| `stdlib/Effect/snapshot.json`, `stdlib/Effect/surface/` | Reviewed Effect-surface evidence, not automatic support |
| `runtime-go/go.mod`, `runtime-go/go.sum`, `runtime-go/flickpg/modules.txt`, `runtime-go/flickcompress/modules.txt` | Driver closure and generated-app dependency filtering |
| `rust-toolchain.toml`, `formal/lean-toolchain` | Toolchain selection, separate from application runtime requirements |

For actual dependency changes, use the existing [validation matrix](../../docs/agents/validation.md), capability checks, adapter surface/conformance tests, and affected runtime suites. None was run for these read-only measurements. Return to the [reference index](index.md).
