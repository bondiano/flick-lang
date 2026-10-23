#!/usr/bin/env node
// Runs the Flick program of `src/`, built into `dist/` by `pnpm build`.
// Without arguments it is the `.d.ts` reader `flick ffi import` runs, with
// `{"root": "<project dir>", "package": "<name>"}` on stdin; `--help` lists
// the other commands.
import { runMain } from "@flick-lang/runtime";

const { main, import_main } = await import("./dist/Main.js").catch((error) => {
  process.stderr.write(`@flick-lang/dts-importer is not built: run \`pnpm -C tools/dts-importer build\` (${error.message})\n`);
  process.exit(1);
});

await runMain(process.argv.length > 2 ? main : import_main);
