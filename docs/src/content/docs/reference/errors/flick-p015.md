---
title: "FLICK-P015"
---

The installed `@flick-lang/runtime`, `@flick-lang/effect-ts` or `effect` is not the version this
compiler was built for, so their ABI may differ. `check`, `build`, `run` and
`test` refuse to go on rather than produce code that misbehaves. Install the
versions the project template pins.

```sh
flick add @flick-lang/runtime@0.1.0 @flick-lang/effect-ts@0.1.0 effect@4.1.2
flick install
```
