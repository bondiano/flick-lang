// Adds the published component of a backend to the backends index the
// compiler resolves official backends from, replacing an entry of the same
// version: `release-index.mjs <backend> <url> <index>`, the index read if it
// exists and written in place.
import assert from "node:assert/strict";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const [backend, url, file] = process.argv.slice(2);
assert.ok(/^[a-z]+$/.test(backend ?? "") && url && file, "usage: release-index.mjs <backend> <url> <index>");
const pin = JSON.parse(readFileSync(join(root, `backends/${backend}/package/backend.json`), "utf8"));
assert.ok(pin.backend_api && pin.ir, `backends/${backend}/package/backend.json must name what the component accepts`);
const entry = {
  id: pin.identity.id,
  version: pin.identity.version,
  sha256: pin.sha256,
  url,
  backend_api: pin.backend_api,
  ir: pin.ir,
};
const index = existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : { backends: [] };
const parts = (version) => version.split(/[.-]/).map((p) => (/^\d+$/.test(p) ? Number(p) : p));
const order = (a, b) => {
  const [x, y] = [parts(a.version), parts(b.version)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    if (x[i] !== y[i]) return x[i] === undefined ? -1 : y[i] === undefined ? 1 : x[i] < y[i] ? -1 : 1;
  }
  return 0;
};
index.backends = index.backends
  .filter((e) => !(e.id === entry.id && e.version === entry.version))
  .concat([entry])
  .sort((a, b) => a.id.localeCompare(b.id) || order(a, b));
writeFileSync(file, `${JSON.stringify(index, null, 2)}\n`);
console.log(`${entry.id}@${entry.version}#${entry.sha256}: indexed`);
