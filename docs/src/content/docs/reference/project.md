---
title: "Project"
---

A project is a directory with `flick.toml` (see [The Flick CLI](../../spec/12-cli/)). Commands find the nearest
`flick.toml` from the working directory upwards; `--project <path>` selects a
project directory or a configuration file explicitly.

## `flick.toml`

```toml
schema-version = 1

[project]
kind = "app"
source-dir = "src"
test-dir = "test"
entry = "App.Main"

[build]
out-dir = "dist"
profile = "debug"
emit = "js"
emit-dts = false
promise-facade = false
schema-aot = false

[sql]
dialect = "postgresql"

[lint]
allow = ["FLICK-T021"]
```

| Key | Values | Default |
| - | - | - |
| `schema-version` | `1`; required, checked before anything else | — |
| `project.kind` | `"app"` or `"lib"`; required | — |
| `project.source-dir` | directory of source modules | `"src"` |
| `project.test-dir` | directory of test modules | `"test"` |
| `project.entry` | module that exports `main`; required for `app` | — |
| `build.out-dir` | base directory of artifacts | `"dist"` |
| `build.profile` | `"debug"` or `"release"` (see [codegen.md](../codegen/)); `--profile` of `build` and `test` overrides it | `"debug"` |
| `build.emit` | `"js"` or `"ts"` | `"js"` |
| `build.emit-dts` | boolean | `false` |
| `build.promise-facade` | boolean | `false` |
| `build.schema-aot` | boolean | `false` |
| `build.solver-steps` | instance resolution steps per definition | `100000` |
| `build.solver-depth` | depth of nested class constraints | `256` |
| `build.labels` | what a release build labels `Module.name` for debugging tools: `"all"` (atoms, views, actions), `"components"` (views and actions) or `"none"`; debug labels all; `flick build --labels` overrides it | `"all"` |
| `sql.dialect` | the SQL dialect of `sql table` and `sql { … }` (see [sql.md](../sql/)): `"postgresql"`; `"mysql"`, `"sqlite"`, `"clickhouse"` and `"oracle"` are recognized but not supported yet | `"postgresql"` |
| `lint.allow` | warning codes not reported, such as `"FLICK-T021"`; errors cannot be turned off | `[]` |

Errors in the configuration exit with code `2`:

| Problem | Diagnostic |
| - | - |
| TOML syntax, an unknown key, a value of the wrong type or outside its set | `FLICK-P005` |
| a `schema-version` other than `1`, or none | `FLICK-P006` |
| `kind = "app"` without `entry` | `FLICK-P007` |
| no project found, or `--project` names none | `FLICK-P008` |
| `entry` is not a source module that exports `main` | `FLICK-P011` |
| `sql.dialect` names a dialect that is not supported yet | `FLICK-P022` |
| `sql.dialect` names no dialect; the message lists the known ones | `FLICK-P023` |
| `lint.allow` names an unknown code | `FLICK-P005` |

## `flick check`

Without paths `flick check` checks every module of the source and test roots.
With paths it checks those files and directories, together with every module
they import, transitively. Outside a project paths are required.

## Packages

Package names, versions and dependencies live in `package.json`; pnpm installs
them. Flick only reads the installed tree: for each dependency of
`package.json` it reads `node_modules/<name>/package.json`. A package with Flick
modules names the directory of their interfaces:

```json
{ "name": "@acme/text", "flick": { "iface-dir": "iface" } }
```

Module `Acme.Text` of that package is described by `iface/Acme/Text.iface`;
the format is in [iface.md](../iface/).

| Problem | Diagnostic |
| - | - |
| an imported module is found nowhere while a dependency is not installed | `FLICK-P009`: run `flick install` (exit code `2`) |
| an imported module is found nowhere | `FLICK-R003`: add the package with `flick add <package>` |
| an `.iface` file is malformed, has another `iface-version` or language version, or describes another module | `FLICK-P010` (exit code `2`) |
| a package module was built against another interface of a dependency than the installed one | `FLICK-P014` |
