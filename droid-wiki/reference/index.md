# Reference

Active contributors: bondiano, Vassiliy — non-bot author names on `origin/main`; names may be aliases.

These pages index the configuration, shared data contracts, and direct dependencies that connect Flick's compiler, tools, plugins, and runtimes. They describe the inspected checkout, not a separately verified release.

## Find a contract

| Reference | Questions answered |
| - | - |
| [Configuration](configuration.md) | Which manifest, CLI, Cargo, and environment settings apply? What are their actual defaults? |
| [Data models](data-models.md) | How do source positions, interfaces, checker metadata, IR, Hosts, editor analysis, and plugin facts relate? |
| [Dependencies](dependencies.md) | How many direct declarations does each package have? Which requirements are optional, target-gated, or runtime-specific? |
| [Subsystem ownership map](../maintainers.md) | Which default-branch contributors have worked in each subsystem? |

For control flow rather than field-level contracts, start with [architecture](../overview/architecture.md), [the compiler](../systems/compiler/index.md), [project and CLI](../systems/project-and-cli/index.md), or [runtimes](../packages/runtimes/index.md).

## Version numbers are different contracts

| Surface | Inspected value | Source |
| - | - | - |
| Project manifest | `schema-version = 1` | `flick-core/src/project/config/parse.rs` |
| Published module interface | `iface-version = 11`, exact compiler package version | `flick-core/src/iface.rs` |
| Serialized whole-program IR | `0.5.0` | `flick-ir/src/program.rs` |
| Plugin SDK/component interface | `0.2.0` | `flick-plugin/src/lib.rs`, `flick-plugin/wit/plugin.wit` |
| Compiler/runtime package versions | `0.1.0` | `flick/Cargo.toml`, `runtime/package.json`, `packages/effect-ts/package.json` |

An equal package version does not replace the interface-version check, and plugin syntax arenas are not backend IR.

## Snapshot and measurement limits

- Source inspection uses the **2026-11-10 working tree on `main` at `6e9096fb`**, including pre-existing uncommitted changes. Contributor attribution instead uses the default branch, `origin/main`.
- In particular, `Cargo.toml` and `flick-core/Cargo.toml` are modified; `packages/effect-ts/tools/surface/`, `runtime/rt_exports.txt`, and `runtime/rt_stubs.txt` are uncommitted. Their descriptions and counts are working-tree evidence, not claims about the base commit.
- Dependency counts parse manifests directly. They are neither resolved lockfile counts nor measured binary/bundle footprints. The [dependency method](dependencies.md#counting-method) states the handling of repeated names and optional features.
- This reference adds prose only. It does not change syntax, execution semantics, model statements, or Lean proof obligations. No compiler/runtime suite was run to generate it.

## Key source files

| Source | Reference boundary |
| - | - |
| `flick-core/src/project/config.rs`, `flick-core/src/project/config/parse.rs` | Shared project configuration and native parsing |
| `flick/src/args.rs`, `flick/src/args/tools.rs` | CLI options and command defaults |
| `flick-core/src/source.rs`, `flick-core/src/iface.rs`, `flick-core/src/types/typing.rs` | Source identity, import contracts, checker decisions |
| `flick-ir/src/lib.rs`, `flick-ir/src/program.rs` | Backend representation and persistence |
| `flick-core/src/project/mod.rs`, `flick-core/src/project/load/ide.rs` | Host and analysis APIs |
| `Cargo.toml`, `pnpm-workspace.yaml`, `runtime-go/go.mod` | Package membership and dependency channels |
