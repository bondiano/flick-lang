// Startup time of a server: from spawning it to its first answer of 200 at
// `--url`, the median of `--runs` starts. The command is everything after
// `--`; `PORT` is set from the URL.
//
//   node tools/load/startup.mjs --url http://127.0.0.1:3000/item/x [--runs 10] -- dist/go/flick-server
import { spawn } from "node:child_process";
import { parseArgs } from "node:util";

const { values: options, positionals: command } = parseArgs({
  options: { url: { type: "string" }, runs: { type: "string", default: "10" } },
  allowPositionals: true,
});
if (!options.url || command.length === 0) {
  console.error("usage: startup.mjs --url <url> [--runs N] -- <command…>");
  process.exit(2);
}
const port = new URL(options.url).port;

const once = async () => {
  const started = performance.now();
  const child = spawn(command[0], command.slice(1), { env: { ...process.env, PORT: port }, stdio: "ignore" });
  try {
    for (;;) {
      try {
        const response = await fetch(options.url);
        await response.text();
        if (response.status === 200) return performance.now() - started;
      } catch {
        // not listening yet
      }
      await new Promise((resolve) => setTimeout(resolve, 2));
    }
  } finally {
    child.kill();
    await new Promise((resolve) => child.once("exit", resolve));
  }
};

const times = [];
for (let i = 0; i < Number(options.runs); i++) times.push(await once());
times.sort((a, b) => a - b);
console.log(`startup to first answer: median ${times[times.length >> 1].toFixed(1)} ms of ${times.length}`);
