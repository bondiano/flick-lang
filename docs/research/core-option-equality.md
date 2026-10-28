# Packages on `core`: can one build serve both backends?

Researched 2026-10-28 at `a2c780d`. Question: a library built on Effect also writes `_core/`, a copy of the modules that need no Effect, for apps with `effect = "none"`. Could one build serve both if the Effect and `core` data were interchangeable?

**Citation keys**
- `fc:` = `flick-core/src/`, `rt:` = `runtime/src/`
- `M:` = a measurement made for this note with Node 24 and `effect@4.0.5` from the workspace

## 1. Equality and hash of `Option`/`Result`

- `core` builds `Some`/`Success` as plain objects with Effect's field names (`rt:core/data.ts:44-47`). Its `equals` compares plain objects key by key, but only when both have the same prototype (`rt:core/data.ts:57-67`). Its `hash` is its own FNV-like fold over sorted keys (`rt:core/data.ts:91`).
- On Effect, `==` is `Equal.equals` and `Hash.hash` (`rt:data.ts:35-37`). The codegen emits `rt("equals")(l, r)` for either backend (`fc:codegen/js/binary.rs:55-56`).
- M: `Equal.equals(Option.some(1), {_tag: "Some", value: 1})` is `false`, in both argument orders. An Effect value implements `Equal`; a plain one does not; and `Equal.equals` never equates the two.
- M: `Hash.hash(Option.some(1))` = 1546030207 and `Hash.hash({_tag: "Some", value: 1})` = 817926444.
- M: `Option.isOption` and `Result.isResult` of the plain values are `false`, so Effect's own `Option`/`Result` functions reject `core` data.

`Eq`/`Hash` instances for `Option`/`Result` match on the constructor (`fc:types/builtins/Instances.fl`), so code that goes through a dictionary would work with either shape. `==`, hashing in `HashMap`/`HashSet`, and Effect's functions would not.

## 2. Equality is not the only reason

A library module built for Effect imports `@flick-lang/runtime`, whose `data.ts` imports `effect` (`rt:data.ts:4`). The same module in `_core/` imports `@flick-lang/runtime/core`, which has no `effect` import (M: `tools/dts-importer/dist/Bump.js:1-2` against `_core/Ts.js:1-2`). An app with `effect = "none"` exists to ship without Effect. Even with interchangeable data, importing the Effect build would bring Effect into the bundle. The modules need their second emission whatever `==` does.

## 3. Alternatives considered

| Option | Cost | Verdict |
|---|---|---|
| `core` builds Effect-tagged objects (`Equal`/`Hash` symbols, Effect's prototype) | Needs Effect's `Equal`, `Hash` and the `Option`/`Result` prototypes in `core`, which is what `none` removes. The 0.4 KB gzip `none` program would grow by the shared Effect kernel | Rejected |
| `rt.equals`/`rt.hash` on Effect treat a plain `{_tag}` like the Effect value | Every `==` pays an extra branch. The hash still differs unless Effect's own hash is reproduced. `Option.isOption` still rejects plain values. Does not solve §2 | Rejected |
| Keep the dual build (`_core/` plus `abi.core`; `P027` for a `none`-only library in an Effect app) | Extra files in the package | Kept |

## 4. Size of `_core/`

- `examples/` has only applications (`kind = "app"`), so it has no `_core/` to measure.
- M: `tools/dts-importer`, the one library in the repository: `_core/` is `Ts.js`, 5,801 bytes (plus a 380-byte map). The top-level modules are 43,951 bytes (`dist/*.js`), so `_core/` adds about 13%. Only modules that need no Effect are copied (`fc:build/effects.rs:13`, `CORE_DIR`).

## 5. Decision

The dual build stays. `core` and Effect data are not equal under `==` or hash. Even if they were, the Effect build of a module imports Effect, which an app without Effect must not load. The cost is a copy of the modules that need no Effect, about 13% of the one library measured.
