# tools

Node scripts and small packages that generate, build and measure for the
rest of the repository. Most run through a recipe of the
[justfile](../justfile), whose comment says what it does.

## Layout

- `go-*/` — write tables and test data into `runtime-go/flickrt`
  (`just go-oracles`).
- `rt-exports.mjs` — writes the runtime's RT manifests from
  `runtime/dist` (see [runtime](../runtime/AGENTS.md)).
- `backend-package/`, `component-build/` — reproducible official backend
  components (`just check-component <js|go>`,
  `just update-component <js|go>`); see `backend-package/README.md`.
- `perf/`, `bundle-size/` — budgets in `perf/budgets.json` and
  `bundle-size/programs.mjs`, held by `just perf`, `just bundle-size` and
  `just ratchet`.
- `dts-importer/` — `.d.ts` reader for `flick ffi import` and
  `just effect-bump`; a Flick project built with this checkout's compiler.
- `astro-flick/` (published as `@flick-lang/astro`), `content-mapper/` —
  `.fl` modules in Astro/Vite and TypeScript, used by the site.
- `style/`, `wasm-dedup/`, `language-rt/` — steps of `just lint`,
  `just wasm` and `just language`.

## Rules

- Prefer the recipe to a direct call: it passes the right compiler build
  and `FLICK_STD_PATH`.
- Run `tools/go-*` only under the Node of `.tool-versions`; another ICU
  writes different tables, and the recipe refuses it.
- A looser budget needs a `reason` beside it, or `just ratchet` fails.
- `.fl` files here are in the formatting corpus; format them with
  `flick fmt`.
- `dts-importer/test/fixtures/*/dist/` are tracked `.d.ts` fixtures, not
  build output.

## Checks

- A workspace package with a `test` script: `pnpm -C tools/<name> test`.
- Other tests: `node --test tools/<name>/*.test.mjs`.
- Then the recipe that runs the tool and the matching row of the
  [validation matrix](../docs/agents/validation.md).
