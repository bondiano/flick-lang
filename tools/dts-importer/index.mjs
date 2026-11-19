#!/usr/bin/env node
// Runs the Flick program of `src/`, built into `dist/` by `pnpm build`:
// `import` is the `.d.ts` reader the `ffi import` command of the JavaScript
// backend runs; `--help` lists the others. It runs as a `main` of
// `Runner.Node` does, with the live host services.
import { run } from "@flick-lang/effect-ts/runner/node";

const { main } = await import("./dist/Main.js").catch((error) => {
  process.stderr.write(`@flick-lang/dts-importer is not built: run \`pnpm -C tools/dts-importer build\` (${error.message})\n`);
  process.exit(1);
});

await run(main);
