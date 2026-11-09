---
title: "FLICK-P031"
---

A workspace or a path dependency cannot be used. A `flick.toml` with
`[workspace]` and no `[project]` is the root of a workspace, not a project:
commands run there go through every member, or the one `--package <name>`
names. A dependency `[dependencies] name = { path = "…" }` names a directory
with a `flick.toml` of `kind = "lib"`, whose sources the project compiles;
dependencies must not form a cycle.

```toml
# the root of the workspace
schema-version = 1

[workspace]
members = ["contract", "server"]
```

```toml
# server/flick.toml
[dependencies]
contract = { path = "../contract" }
```
