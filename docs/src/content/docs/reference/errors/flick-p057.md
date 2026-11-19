---
title: "FLICK-P057"
---

The core manages the Flick libraries of `[dependencies]`: `flick add` with
`--git`, `--url` or `--path` adds one. Every other package belongs to the
ecosystem of the project's backend, which implements `add`, `remove`,
`update`, `install` and `deps` for it as commands of its own. This project's
backend has no such command, or not one that takes this option, so `flick`
has nowhere to send the package.

Name a Flick library with a source option, or build with a backend whose
ecosystem has the package.

```sh
flick add geometry --git https://example.com/geometry.git --tag v1.2.0
flick add geometry --path ../geometry
```
