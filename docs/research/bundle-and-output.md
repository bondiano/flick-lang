# Bundle weight and readable output: Flick measured, ReScript as the reference

Researched 2026-10-24 at `8e7b3a5` (branch `hetzner/flick-lang`). Goal: smaller browser bundles and JS a person can read, for the UI redesign on `effect/reactivity` atoms and React 19.

**Citation keys**
- `fc:` = `flick-core/src/`, `rt:` = `runtime/src/`, `cg:` = `docs/src/content/docs/reference/codegen.md`, `spec11:` = `docs/src/content/docs/spec/11-compiler.md`
- `rs:` = github.com/rescript-lang/rescript at `master` `d53b8ae6`, read with `gh api`. `rsdoc:` = rescript-lang.org/docs/manual/… (v12 manual)
- `M:` = a measurement made for this note. The scripts and outputs are in the session scratchpad and are not committed (see Method)
- **[unverified]** = inferred, not measured or quoted

## 1. Measurements

### Method

- Compiler: `/var/cache/cargo-target/debug/flick build --profile debug|release`. The runtime was built from this worktree's `runtime/src` with `tsc -p tsconfig.build.json` into a scratch copy. This worktree has no `node_modules`.
- Bundler: no esbuild in the workspace. I used **rolldown 1.2.15**, the engine of Vite 8 that the docs site uses, taken read-only from the `flick-lang-diag` worktree's pnpm store. It also supplied `effect@4.0.5`, `react@19.3.1` and `react-dom@19.3.1`. Settings: `platform: "browser"`, ESM, one chunk, `process.env.NODE_ENV = "production"`, oxc minifier (`minify: true`), gzip at the default level.
- Externals: `node:*`, `jsdom` and the host foreign modules (`dom.mjs`, `dom.ts`, `compiler.ts`, `highlight.ts`). Host code is not Flick output.
- Breakdown: each byte of the minified bundle was attributed to its source through the bundle's source map, the way source-map-explorer does it. A group's gzip figure is gzip of that group's bytes alone, so the groups do not add up to the total. `unmapped` is code rolldown generated, mostly namespace objects.
- Programs:
  - `pure`: an ADT, `match`, `add`, one `Platform.stdout_line`.
  - `counter`: `tests/run/ui/counter.fl`.
  - `play`: `docs/src/playground/Playground.fl` and its 8 modules.
- Entries:
  - **A**: the emitted `flick-main.js` (`runMain(main)`), which is what `Playground.astro` ships.
  - **B**: `Effect.runFork(main(undefined))`, with no entry runner.
  - **C**: B plus `"sideEffects": false` in the runtime's `package.json`. This was applied only to the scratch copy.

### Results (KB; group cells are min / gz)

| program | entry | profile | raw | min | **gz** | Flick app | `_flick` builtins | runtime | effect | react | unmapped |
|---|---|---|---|---|---|---|---|---|---|---|---|
| pure | A | debug | 4320.1 | 813.2 | **239.3** | 0.3/0.2 | 0.1/0.1 | 6.2/3.0 | 730.5/209.4 | – | 75.9/23.1 |
| pure | A | release | 4320.1 | 813.2 | **239.3** | 0.3/0.2 | 0.1/0.1 | 6.2/3.0 | 730.5/209.4 | – | 75.9/23.1 |
| pure | B | release | 294.2 | 78.6 | **25.2** | 0.3/0.2 | 0.1/0.1 | 2.8/1.4 | 75.2/23.7 | – | 0.2/0.2 |
| pure | C | release | 89.0 | 22.6 | **8.0** | 0.3/0.2 | 0.1/0.1 | 0.8/0.5 | 21.3/7.4 | – | – |
| counter | A | debug | 4896.4 | 1040.7 | **309.5** | 1.7/0.7 | 0.5/0.3 | 15.3/6.4 | 730.8/209.5 | 214.9/66.0 | 76.4/23.4 |
| counter | A | release | 4896.4 | 1040.7 | **309.5** | 1.7/0.7 | 0.5/0.3 | 15.3/6.4 | 730.8/209.5 | 214.9/66.0 | 76.4/23.4 |
| counter | B | release | 905.9 | 310.3 | **96.8** | 1.6/0.7 | 0.5/0.3 | 12.6/5.3 | 79.7/25.0 | 214.1/66.0 | 0.7/0.4 |
| counter | C | release | 726.3 | 259.3 | **81.2** | 1.6/0.7 | 0.5/0.3 | 10.8/4.5 | 30.7/10.2 | 214.1/65.9 | 0.6/0.3 |
| play | A | debug | 4924.6 | 1059.1 | **315.8** | 16.0/5.2 | 1.0/0.4 | 16.7/7.0 | 730.9/209.5 | 214.9/66.1 | 76.7/23.4 |
| play | A | release | 4925.1 | 1059.2 | **315.9** | 16.0/5.3 | 1.0/0.4 | 16.7/7.0 | 730.9/209.6 | 214.9/66.1 | 76.7/23.4 |
| play | B | debug | 947.6 | 332.2 | **104.1** | 15.7/5.1 | 0.9/0.4 | 13.2/5.5 | 84.3/26.3 | 214.2/66.0 | 1.0/0.4 |
| play | B | release | 948.1 | 332.3 | **104.1** | 15.8/5.2 | 0.9/0.4 | 13.2/5.5 | 84.3/26.3 | 214.2/66.0 | 1.0/0.4 |
| play | C | release | 793.5 | 288.5 | **90.9** | 15.7/5.2 | 0.9/0.4 | 11.5/4.8 | 42.5/13.9 | 214.1/65.9 | 0.8/0.3 |

Reference points (M:)
- **Floor**: `Effect.runFork(Effect.gen(… Effect.sync …))` written by hand is 21.4 min / **7.4 gz**.
- **Atoms**: a minimal `effect/reactivity` program (`Atom.make`, a derived atom, `AtomRegistry.make`, `get`/`set`) is 78.2 min / **25.1 gz** on its own. How much of that overlaps with Flick's Effect core is **[unverified]**.
- **Entry A with the runtime marked side-effect free**: pure 236.9, counter 307.2, play 313.6 gz. Only the runner change removes the bulk.
- **Output size on disk**: the play app modules are 36 989 B in release and 36 286 B in debug. Every build also writes **143 `_flick/*.js` files (578 162 B)**, whatever the program imports. The pure program imports 3 of them.

## 2. Findings

### 2.1 Bundle weight

1. **`runMain` keeps the whole of Effect: about 213 KB gz, two thirds of every bundle.** `rt:entry.ts:61` calls `capabilities()`. That function (`rt:capabilities.ts:4,67`) does `import * as effect from "effect"` and looks up `MATRIX` entries by dynamic keys. A bundler must therefore keep every module behind the barrel (216 effect modules, M:). Entry B removes it: counter drops from 309.5 to 96.8 gz. The docs playground ships entry A through `docs/src/components/Playground.astro`.
2. **The runtime declares no `sideEffects`.** `runtime/package.json` has no such field, while effect has a list of two files (`effect/package.json:28`). The runtime also has top-level calls without `/*#__PURE__*/`, for example `Context.Reference(...)` (`rt:owner.ts`, dist line 9), `Option.none()` and `ordering("LT")` (`rt:data.ts`), and Schema constants under `rt:schema/`. The bundler cannot drop modules re-exported from `rt:index.ts` that are never used, and they pull in Schema, SchemaAST, net and http code (M: module list for entry B). Marking the package side-effect free takes pure from 25.2 to 8.0 gz, counter's Effect share from 25.0 to 10.2, and play from 104.1 to 90.9 (M: entry C). Whether that flag is safe still needs an audit and a test **[unverified]**.
3. **Namespace imports are fine.** `import { Effect } from "@flick-lang/runtime"` and `import * as $rt` (`fc:codegen/js/mod.rs:91`) shake down to the members actually used. The floor of 7.4 gz and entry C's 8.0 gz show this for rolldown. esbuild was not tested **[unverified]**.
4. **The builtin modules shake well. What is wasted is build output, not bundle.** The counter imports 10 of the 143 `_flick` modules (`Database.js` is not among them) and keeps 0.5 KB of them (M:). Each build still writes all 143 (`fc:build/mod.rs:141-154`).
5. **Emitted modules have top-level calls without pure annotations.** There is no `__PURE__` anywhere in the output (M: grep). Two examples:
   - `export const $i_Show_Refused_4efda98c = $rt.derive("Show", …)` is emitted by `fc:codegen/js/items.rs:309-327`. It stays in the minified counter bundle although nothing uses it: the bundle contains `zd("Show",()=>[qf],[["Refused#7de0d8b8de9e",…]])` (M:).
   - `export const initial_source = (() => {…})()` in `State.js` is a top-level IIFE, so it always runs.

   `spec11:60` already requires that "Top-level source initialization is pure", so the compiler may state that with annotations. The emitted `package.json` is only `{ "type": "module" }` (`fc:build/mod.rs:136-139`).
6. **Release is not smaller than debug.** Play's app code is 16.0 min in both profiles and 37.0 KB raw in release against 36.3 KB in debug. An exported function keeps a curried wrapper beside its worker, `export const update = ($a0) => ($a1) => update$2($a0, $a1)` (`fc:codegen/js/release.rs:16-49`). Importers still call the curried form, because workers are private to their module.
7. **Docs builds use debug by default.** See `tools/astro-flick/vite.mjs:31`, `profile ?? "debug"`, and `docs/astro.config.mjs:23`, which passes no profile.
8. **React is the floor.** `react-dom/client` is 214 min / 66 gz in every UI build. After fixes 1 and 2, the part Flick controls in counter is about 16 gz (app 0.7, builtins 0.3, runtime 4.5, effect 10.2). Atoms will add up to about 25 gz on top of that **[unverified overlap]**.
9. **Source maps are external** (`//# sourceMappingURL=Main.js.map`). Their `sources` hold absolute `file:///…` paths (M: `Pure/Main.js.map`), which leaks build-machine paths into shipped maps.

### 2.2 Readability: what Flick emits, what ReScript emits, idiomatic TS

All Flick excerpts were compiled in release (M:). The ReScript column is written from `rsdoc:variant`, `rsdoc:null-undefined-option` and real compiler test output (`rs:tests/tests/src/variant.mjs`). It was **not compiled here**, because the workspace has no ReScript compiler.

**Pure ADT and match** (`pure`):
```js
export const Circle = (_0) => ({ _tag: "Circle#d930eed2c811", _0 });
export const Rect = (_0) => (_1) => ({ _tag: "Rect#f6d6c50e18b6", _0, _1 });   // curried constructor
export const area = (s) => {
  if (s._tag === "Circle#d930eed2c811") { const r = s._0; return ((3.14 * r) * r); }
  if (s._tag === "Rect#f6d6c50e18b6") { const w = s._0; const h = s._1; return (w * h); }  // last arm still tested
};
export const main = ($_) => Effect.gen(function* () {
  const a$i1 = area(Circle(2));
  const b$i1 = area(Rect(3)(4));                                    // add inlined, Rect still curried
  return (yield* $m_Platform.stdout_line($m_Instances.$i_Show_Number_fbf2eb85.show((a$i1 + b$i1))));
});
```
- In debug, `add` stays `export const add = (a) => (b) => (a + b);` and is called as `add(x)(y)`.
- ReScript emits `{TAG: "Circle", _0: r}`, a constant constructor is the string `"Yes"` (`rsdoc:variant`), and the last case is not tested: `if (typeof x !== "object") …; switch (x.TAG) { case "B": … }` (`rs:variant.mjs`). Two-argument functions compile to "simple JavaScript function calls" (rescript-lang.org/blog/uncurried-mode).
- Idiomatic TS: `function area(s: Shape) { switch (s._tag) { case "Circle": …; case "Rect": … } }`.

**UI view** (`counter`):
```js
const counter$2 = (model, n) => {                 // a release worker, named `$2`
  return $m_Ui.div({ class_name: "counter" })([$m_Ui.heading([$m_Ui.text(("count " + $m_Instances.$i_Show_Number_fbf2eb85.show(n)))]),
    ($m_Ui.button({ disabled: false, on_click: $m_Ui_Event.exhaust(($_) => {
      return Effect.gen(function* () { return (yield* $m_Ref.set(model)((n + 1))); });   // gen around one yield
    }) }))([$m_Ui.text("+1")]), …
```
- Every element is `f(attrs)(children)`, a curried call across modules.
- The click handler builds a generator, its iterator, and an extra `Effect` around a single effect.
- Idiomatic TS would be `div({ className: "counter" }, [heading([text(\`count ${n}\`)]), button({ onClick: exhaust(() => Ref.set(model, n + 1)) }, …)])`.

**Equality and enums** (`play/State.js`):
```js
export const Ru = { _tag: "Ru#c773eaf21a84" };
export const tr = (locale) => (english) => (russian) => { if ($rt.equals(locale, Ru)) { return russian; } else { return english; } };
export const errors = (result) => $m_List_Core.length(($m_List_Core.filter((d) => $rt.equals(d.severity, "error")))(result.diagnostics));
export const $i_Eq_Locale_3ee01fcb = $rt.derive("Eq", () => [], [["En#c799e1f21aa5", "En", 0], ["Ru#c773eaf21a84", "Ru", 0]], {});
```
- `==` always lowers to `$rt.equals`, which is Effect's `Equal.equals` (`fc:codegen/js/expr.rs:349-350`, `rt:data.ts:52`). That holds even for a `String` against a literal.
- ReScript compares an enum with `x === "Ru"`, and a structural compare goes through `Primitive_object.equal` (`rs:variant.mjs`).

**Option, Result, records and naming**
- Option and Result are Effect's native objects: `found._tag === "None"` and `found.value` (`cg:39`, `fc:codegen/names.rs:130`). ReScript emits `Some(5)` as `5` and `None` as `undefined` (`rsdoc:null-undefined-option`).
- Records are plain objects in both languages (`cg:34`; `rsdoc:record`: "compile to straightforward JavaScript objects"). Updates use `{ ...m, … }`.
- Flick's generated names:
  - `$m_<Module>` for imports (`fc:codegen/names.rs:78`)
  - `$i_<Class>_<Type>_<fnv32>` for instances (`names.rs:83-92`)
  - `<Name>#<48-bit fnv>` for tags (`names.rs:99-104`)
  - `$f_` for foreign imports, `$t1` for destructuring temps, `a$i1` for inlined locals, `f$2` for workers
- ReScript keeps source names and imports modules as `import * as Primitive_object from …`.

## 3. ReScript techniques and how they map to Flick

| # | Technique (citation) | Flick today | Applicable? | Size / readability | Cost / risk |
|---|---|---|---|---|---|
| R1 | **Uncurried by default, arity-aware calls.** In v11, calls "get compiled as simple JavaScript function calls" and partial application is explicit, `add(5, ...)` (blog/uncurried-mode, `rsdoc:function`) | Curried per stage (`fc:codegen/js/function.rs:43-136`, `cg:78`). Release has workers for full applications within a module only (`fc:codegen/js/release.rs:16-49`) | Partly. Stage semantics stay: a runtime first stage must finish before the next argument (`cg:100`). Exported workers plus arity in `.iface` give cross-module `f$2(a, b)` | Medium: 248 `)(` chains in play debug, 196 in release (M:). High readability gain | Medium. The TS projection is "one arrow per stage" (`cg:164`), so keep the curried export and add the worker |
| R2 | **Records as plain objects** (`rsdoc:record`) | Done (`cg:34`) | – | – | – |
| R3 | **Constant constructors as strings, `TAG` objects otherwise, `@tag` and `@as`** (`rsdoc:variant`, `rs:variant.mjs`) | Objects `{ _tag: "Play#…" }`, including nullary ones (`fc:codegen/js/items.rs:203-224`). The tag carries a hash (`names.rs:99-104`) | A nullary constructor could be its tag string, matched with `typeof x === "string"`. `_tag` already matches Effect's convention, so `@tag` is not needed | Small bytes (gzip folds repeats; 299 B of hashes in play, M:). Large readability gain if the hash is dropped where it is not needed | Medium. `rt:data.ts` `derive`/`show`/`equals` read `_tag` reflectively. Cross-package identity is why the hash exists (`cg:42-48`) |
| R4 | **`@unboxed`** (`rsdoc:variant`, "Untagged Variants") | Done: `unboxed type` and `untagged type` (`cg:50-66`, `Rep::Unboxed`/`Typeof` in `items.rs:211-214`) | – | – | – |
| R5 | **Option as `undefined`** (`rsdoc:null-undefined-option`; nested options use `Primitive_option.valFromOption`, `rs:compiler/core/js_of_lam_option.ml:65-68`) | Effect `Option` objects (`cg:39`) | Poorly. It breaks the `Option.Option` TS projection (`cg:163`), Effect interop and nested options | Small | High. Not recommended |
| R6 | **No-runtime externals**, "inlined into their callers … completely disappear from the JS output" (`rsdoc:external`) | A `foreign` gets a wrapper per stage, e.g. `eq_prim = ($a0) => ($a1) => $f_eq_prim($a0, $a1)` in `_flick/Instances.js` (M:). `foreign sync` lowers directly to `Effect.sync` (`_flick/Platform.js`) | Yes for `foreign pure` full applications in release: call `$f_x(a, b)` at the use site | Small to medium. Large readability gain in builtin-heavy code | Low |
| R7 | **Constant folding and inlining of small functions**, cross-module, `max_inline_depth = 4` (`rs:compiler/core/lam_pass_remove_alias.ml:49-54`) | Within a module only: `SMALL = 8` and `ONCE = 60` IR nodes (`fc:opt/inline.rs:13-18`), `fold.rs`, `sroa.rs` | Cross-module needs IR bodies in `.iface` | Medium | Medium (`.iface` format, invalidation) |
| R8 | **Module-level DCE**: `js_shake` keeps exports, side-effecting statements and what they reach (`rs:compiler/core/js_shake.ml`) | Private unused values are dropped (`fc:opt/mod.rs:216`). Exports rely on the bundler | Yes, by making the bundler effective (R9) | See R9 | Low |
| R9 | **ESM with `/* @__PURE__ */` on pure runtime calls** (`rs:compiler/core/js_exp_make.ml:201-204`, `rs:compiler/ext/literals.ml:82`; output `let EA1 = /* @__PURE__ */Primitive_exceptions.create(…)`, `rs:variant.mjs:97`) | ESM yes. Zero pure annotations and no `sideEffects` field (finding 5) | Yes: `spec11:60` already requires pure initialization | Unused derived instances and IIFE constants become removable | Low |
| R10 | **`%raw` / `%%raw`**, pasted "as-is into the output" (`rsdoc:embed-raw-javascript`) | `foreign … from global "…"` (`cg:143`) | Not needed. A raw escape hatch would weaken effect tracking | – | – |
| R11 | **Readable names preserved** (`rs:variant.mjs`: `foo`, `match`, `Path`) | Mostly preserved, with prefixes and hashes (2.2) | Yes: `$i_Show_Number` with the hash only on collision; workers named `f$2` are fine | Readability | Low. Hashed instance names exist to avoid collisions; keep a deterministic suffix only on a clash |
| R12 | **genType**: `.gen.tsx` files, "does not generate anything runtime-related" (`rsdoc:typescript-integration`) | Done better: `--emit ts`, `--emit-dts`, `--promise-facade` (`cg:152-192`) | – | – | – |

### Flick-specific ideas

| # | Idea | Status / evidence | Applicability under Flick semantics | Impact | Risk |
|---|---|---|---|---|---|
| F1 | **Entry runner without the dynamic capability check** | `rt:entry.ts:61`, `rt:capabilities.ts:4` | Move the check to `flick build`/`flick doctor`/tests, or check with static named imports. Effect is pinned at exactly 4.0.5 anyway (`contributing/implementation-decisions.md:15`) | **−213 gz** on every app (M: A→B) | Low |
| F2 | **`sideEffects: false` plus pure annotations in the runtime** | Finding 2 | Audit modules that register at load time | −13 to −17 gz (M: B→C) | Low to medium |
| F3 | **No `Effect.gen` around one tail `yield*`.** `gen(function*(){ return yield* X })` becomes `X` when X calls a Flick runtime function with arguments that cannot throw (locals, literals, lambdas). Otherwise use `Effect.suspend(() => X)` | 14 of 43 gens in play and 5 of 8 in counter are this shape (M:). Emitted at `fc:codegen/js/function.rs:109-114` and `:189-199` | A `->{}` arrow is still a runtime call, because "Empty r and e do not make a runtime call pure" (`spec/03-direct-calls.md`, `spec/04-computations.md` "Purity is separate…"). The value must stay an `Effect`; only the wrapper goes. Deferral is preserved because calling a Flick runtime function only builds a description. Runtime recursion must keep its yield points (`cg:214-216`) | Small bytes, less allocation per run, readability | Low to medium (interruption points, stack traces) |
| F4 | **Pure and `foreign sync` code without `gen`** | Pure code is already plain JS (`cg:79`). `foreign sync` is already `Effect.sync` (M: `_flick/Platform.js`) | A gen over several `Effect.sync` steps can become `Effect.flatMap` chains. Fusing the steps into one `sync` would change interruption granularity, so do not | Readability only | Medium. Lower priority than F3 |
| F5 | **Specialised dictionaries for known instances.** `$m_Instances.$i_Show_Number_fbf2eb85.show(n)` becomes `$rt.show(n)` or a direct export. `==` at a known primitive type becomes `===`, or SameValueZero for Number (`cg:71`). `==` on a nullary-only ADT becomes a `_tag` compare | Release only replaces methods of instances in the same module (`fc:codegen/js/release.rs:94-115`). `==` is always `$rt.equals` (`expr.rs:349`) | The checker knows the instance statically. The rule that a dictionary passed as a value is never replaced (`cg:211`) still applies | Small bytes, large readability and speed | Low |
| F6 | **Exported workers and per-export builtins.** Builtins already shake (finding 4). The gain is calling `Ui.div$2(attrs, kids)` and `Ref.set$2` across modules | `release.rs` | Same as R1 | Medium | Medium |
| F7 | **Write only the reachable `_flick` modules** | All 143 are written (`fc:build/mod.rs:141`) | Disk and build-time only | 578 KB written per build, no bundle change | Low (the LSP, `flick pack` and dynamic imports must still resolve) |
| F8 | **Docs and release defaults.** The Vite plugin should default to release for `build` and debug for `dev` | `tools/astro-flick/vite.mjs:31` | – | Pays off only after R1/F6, since release is currently ≥ debug (finding 6) | Low |
| F9 | **Source maps** | External already. `sources` are absolute `file://` URLs (finding 9) | Write paths relative to the map | Privacy, portability | Low |
| F10 | **Pattern matching**: no test on the last arm of an exhaustive match, `switch` from 3 or more arms on. Constructor applications in release become object literals (`Rect(3)(4)` → `{_tag, _0: 3, _1: 4}`) | `fc:codegen/js/pattern.rs:63` | Exhaustiveness is checked, so the final test is redundant | Small bytes, readability | Low |

## 4. Prioritised improvements

All gains come from the measurements above unless marked. Gzip figures are for the whole bundle.

1. **Browser entry runner without the dynamic capability check (F1).** Acceptance: the counter bundle with entry A, release, is **≤ 97 KB gz** (now 309.5), the pure program is **≤ 26 KB gz** (now 239.3), and no bundle contains `effect/dist/Graph.js` or `Stream.js` unless the app uses them. Expected gain: −213 KB gz, −69 % for counter.
2. **Runtime marked side-effect free (F2).** Add `"sideEffects": false`, or a list, to `runtime/package.json`, and `/*#__PURE__*/` on top-level calls in `rt:`. Acceptance: counter **≤ 82 KB gz**, pure **≤ 9 KB gz**, play **≤ 91 KB gz**, and an e2e or `node --test` check that importing only `Effect` from the runtime keeps no `Schema`. Expected gain: −13 to −17 KB gz, and −60 % of Effect bytes for counter.
3. **Pure output declared and annotated (R9).** The emitted `package.json` gets `"sideEffects": false`. Top-level `$rt.derive(...)`, instance construction and IIFE constants get `/*#__PURE__*/`. Acceptance: the string `Refused#7de0d8b8de9e` is absent from the minified counter bundle when `Show Refused` is unused, and a fixture with an unused `deriving Show, Eq` adds 0 bytes. Expected gain: small per program, but it scales with derived instances **[unverified magnitude]**.
4. **No `Effect.gen` for a lone tail effect (F3).** Acceptance: 0 occurrences of `Effect.gen(function* () {\n return (yield* …);\n})` in counter and play release output (now 5/8 and 14/43), with the e2e corpus passing in both profiles. Expected gain: about 19 fewer generators in these programs, fewer allocations per event handler, and clearer handlers.
5. **Arity-aware calls across modules (R1, F6).** Export `f$n`, record arity in `.iface`, and call workers for full applications, builtins included. Acceptance: in play release, `)(` chains are **≤ 124** (half of debug's 248), app min is **≤ 14.4 KB** (−10 % against debug's 16.0), and release is never larger than debug. Then make the Vite plugin default to release for `build` (F8). Expected gain: about 1–2 KB min in play, more for UI-heavy apps **[unverified]**, and the largest readability gain.
6. **Type-directed `==` and known-instance specialisation (F5, R6).** Acceptance:
   - in play release, no `$rt.equals` where both sides are `String`, `Bool`, `BigInt` or a nullary-only ADT
   - no `$m_Instances.$i_Show_Number_*` or `$i_Show_String_*` references
   - full `foreign pure` applications call `$f_*` directly

   Expected gain: readability, and faster equality in render paths (`$rt.equals` → `===`). Bytes are small.
7. **Readable names (R11, R3).** Instance constants become `$i_Show_Number`, with a hash suffix only on collision. Constructor tags keep their identity hash; whether to drop it in release for types that are private and not exported is open. Acceptance: no 8-hex instance suffix in output for programs without collisions; tag changes stay behind a decision recorded in `implementation-decisions.md`. Expected gain: readability; `Circle#d930eed2c811` costs 13 B per site before gzip.
8. **Match and constructor cleanup (F10).** Exhaustive matches test no last arm, a match with 3 or more arms becomes `switch (s._tag)`, and full constructor applications become literals in release. Acceptance: `area` in `pure` compiles to one `if` and one `return`, or a `switch` with no trailing test, and `Rect(3)(4)` becomes an object literal in release. Expected gain: small bytes, ReScript-level readability for `match`.

Lower priority:
- **F7, builtins on disk.** Acceptance: a pure program build writes **≤ 5** `_flick` files.
- **F9, relative source-map paths.** Acceptance: no `file://` in `*.js.map`.
- **R5, Option as `undefined`.** Not recommended: it conflicts with the Effect `Option` TS projection.
- **R7, cross-module inlining.** Revisit after item 5.

## 5. Not verified

- esbuild and webpack behaviour. Only rolldown 1.2.15 was used.
- Whether `"sideEffects": false` is semantically safe for every runtime module. Entry C's bundles were not run.
- The gzip overlap between `effect/reactivity` atoms and Flick's Effect core.
- ReScript output for the exact Flick programs, which was not compiled. Its shapes come from the docs and `rs:tests/tests/src/variant.mjs`.
- Gains for items 3, 5 and 8, which are estimated from counts and not from rebuilt bundles.
