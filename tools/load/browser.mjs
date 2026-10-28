// Hydrates the streamed pages of the SSR load server in Chromium: `--pages`
// pages of `/suspended/{id}`, `--tabs` at a time, each with a fresh id. A
// page passes when it shows its own id and item, the client ran none of
// its tasks (the server's values were handed over), a click on its button
// is answered (it hydrated) and the page logged no error, a hydration
// mismatch included. Prints the medians of the time to the answered click.
//
//   node tools/load/browser.mjs --url 'http://127.0.0.1:3000/suspended/{id}' \
//     [--pages 500] [--tabs 8] [--timeout 10] [--chromium /path/to/chromium]
import { existsSync } from "node:fs";
import path from "node:path";
import { parseArgs } from "node:util";

import { chromium } from "playwright-core";

const { values: options } = parseArgs({
  options: {
    url: { type: "string" },
    pages: { type: "string", default: "500" },
    tabs: { type: "string", default: "8" },
    timeout: { type: "string", default: "10" },
    chromium: { type: "string", default: process.env.CHROMIUM ?? "chromium" },
  },
});
if (!options.url?.includes("{id}")) {
  console.error("usage: browser.mjs --url <url with {id}> [--pages N] [--tabs N] [--timeout s] [--chromium path]");
  process.exit(2);
}
const timeout = Number(options.timeout) * 1000;
// Playwright takes a path, not a command on PATH
const executable = options.chromium.includes("/")
  ? options.chromium
  : (process.env.PATH ?? "")
      .split(path.delimiter)
      .map((dir) => path.join(dir, options.chromium))
      .find((file) => existsSync(file));
if (executable === undefined) {
  console.error(`no ${options.chromium} on PATH: pass --chromium or set CHROMIUM`);
  process.exit(2);
}

const browser = await chromium.launch({ executablePath: executable });
let next = 0;
const failures = [];
const times = [];

/** Opens one page in `tab` and returns why it failed, if it did. */
const visit = async (tab) => {
  const id = `b${++next}`;
  const errors = [];
  const onConsole = (message) => message.type() === "error" && errors.push(message.text());
  const onError = (error) => errors.push(String(error));
  tab.on("console", onConsole);
  tab.on("pageerror", onError);
  const started = performance.now();
  try {
    await tab.goto(options.url.replaceAll("{id}", id), { timeout });
    const button = tab.locator("#app button");
    await tab.locator("#app").getByText(`item of ${id}`).waitFor({ timeout });
    await button.click({ timeout });
    await tab.locator("#app").getByText("clicks 1").waitFor({ timeout });
    const elapsed = performance.now() - started;
    const heading = await tab.locator("#app h1").textContent();
    const runs = await tab.evaluate(() => globalThis.flickTaskRuns ?? 0);
    if (heading !== `request ${id}`) return `${id}: heading ${JSON.stringify(heading)}`;
    if (runs !== 0) return `${id}: the client ran the task ${runs} times`;
    if (errors.length > 0) return `${id}: ${errors.join("; ")}`;
    times.push(elapsed);
    return undefined;
  } catch (error) {
    return `${id}: ${error.message.split("\n")[0]}${errors.length > 0 ? ` (${errors.join("; ")})` : ""}`;
  } finally {
    tab.off("console", onConsole);
    tab.off("pageerror", onError);
  }
};

const total = Number(options.pages);
const worker = async () => {
  const context = await browser.newContext();
  const tab = await context.newPage();
  while (next < total) {
    // oxlint-disable-next-line eslint/no-await-in-loop -- one page after another in this tab
    const failure = await visit(tab);
    if (failure !== undefined) failures.push(failure);
  }
  await context.close();
};
await Promise.all(Array.from({ length: Number(options.tabs) }, worker));
await browser.close();

times.sort((a, b) => a - b);
const at = (q) => (times[Math.min(times.length - 1, Math.floor(q * times.length))] ?? NaN).toFixed(0);
console.log(`url ${options.url}`);
console.log(`pages ${next}, hydrated ${times.length}, failed ${failures.length}`);
console.log(`to the answered click, ms: p50 ${at(0.5)}, p95 ${at(0.95)}, p99 ${at(0.99)}`);
for (const failure of failures.slice(0, 10)) console.log(`  ${failure}`);
process.exitCode = failures.length === 0 && times.length === total ? 0 : 1;
