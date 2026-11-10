// Startup time of a server: from spawning it to its first answer of 200 at
// `--url`, the median of `--runs` starts. The command is everything after
// `--`; `PORT` is set from the URL.
//
//   node tools/load/startup.mjs --url http://127.0.0.1:3000/item/x [--runs 10] -- dist/go/flick-server
import { startupTimes } from "./server-start.mjs";
import { parseArgs } from "node:util";

const { values: options, positionals: command } = parseArgs({
  options: { url: { type: "string" }, runs: { type: "string", default: "10" } },
  allowPositionals: true,
});
if (!options.url || command.length === 0) {
  console.error("usage: startup.mjs --url <url> [--runs N] -- <command…>");
  process.exit(2);
}
const times = await startupTimes(command, { url: options.url, pollMs: 2 }, Number(options.runs));
times.sort((a, b) => a - b);
console.log(`startup to first answer: median ${times[times.length >> 1].toFixed(1)} ms of ${times.length}`);
