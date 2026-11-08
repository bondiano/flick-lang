---
title: "FLICK-F008"
---

`flick ffi import` runs the Node helper `@flick-lang/dts-importer`, which
resolves the package's declarations from the project the way TypeScript
does. The package must be installed and ship (or have) a `.d.ts`; a
`policy.toml` beside the generated module must name exports the package
has, with the values the importer knows; `--select` must name exports that
exist.

```toml
# src/Ffi/LookupLib/policy.toml
[exports.lookup]
undefined = "none"
catch = "JsError"
```
