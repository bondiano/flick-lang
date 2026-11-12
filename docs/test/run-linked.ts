// Links a build directory as the playground does, with `data:` URLs, and runs it.
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";

import { link, packages } from "../src/playground/loader.ts";

const dir = process.argv[2] ?? ".";
const entry = process.argv[3];
if (entry === undefined) throw new Error("Pass the compiled module's path");
// Network examples run unchanged, against explicit fixtures rather than a live service.
const fetchFixture = process.argv[4];
if (fetchFixture !== undefined) {
  const replies = JSON.parse(readFileSync(fetchFixture, "utf8")) as Record<string, unknown>;
  globalThis.fetch = (input) => {
    const url = input instanceof Request ? input.url : String(input);
    if (!Object.hasOwn(replies, url)) throw new Error(`No fetch fixture for ${url}`);
    return Promise.resolve(Response.json(replies[url]));
  };
}
const files: Record<string, string> = {};
const walk = (sub: string): void => {
  for (const name of readdirSync(path.join(dir, sub))) {
    const rel = sub === "" ? name : `${sub}/${name}`;
    if (statSync(path.join(dir, rel)).isDirectory()) walk(rel);
    else if (rel.endsWith(".js")) files[rel] = readFileSync(path.join(dir, rel), "utf8");
  }
};
walk("");
const store: Record<string, Readonly<Record<string, unknown>>> = {};
for (const spec of packages(files, entry)) store[spec] = (await import(spec)) as Readonly<Record<string, unknown>>;
(globalThis as { __flickPackages?: unknown }).__flickPackages = store;
const url = (code: string): string => `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`;
await import(link(files, entry, url));
