// A Vite plugin for Flick: importing a `.fl` file builds it with the `flick`
// CLI into the cache directory and re-exports the compiled module, whose
// imports (the runtime, the built-in modules, foreign JS next to the source)
// Vite resolves as usual. A change to any `.fl` beside it rebuilds.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";

/** The `.fl` files under `dir`, without `node_modules`. */
const sources = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return entry.name === "node_modules" || entry.name.startsWith(".") ? [] : sources(full);
    return entry.name.endsWith(".fl") ? [full] : [];
  });

/** The module a source declares in its header. */
export const moduleName = (text) => /^module\s+([A-Z][\w]*(?:\.[A-Z][\w]*)*)/m.exec(text)?.[1];

/**
 * @param {{ binary?: string, profile?: "debug" | "release", outDir?: string }} options
 * `binary` is the `flick` CLI (`FLICK_BIN`, else `flick` on the PATH);
 * `outDir` holds the builds (by default `<vite cache>/flick`).
 */
export const flickVite = (options = {}) => {
  const binary = options.binary ?? process.env.FLICK_BIN ?? "flick";
  const profile = options.profile ?? "debug";
  let outBase = options.outDir;
  return {
    name: "flick",
    enforce: "pre",
    configResolved(config) {
      outBase ??= path.join(config.cacheDir, "flick");
    },
    load(id) {
      const file = id.split("?")[0];
      if (!file.endsWith(".fl")) return null;
      const dir = path.dirname(file);
      const all = sources(dir);
      for (const source of all) this.addWatchFile(source);
      // a build per state of the sources, so an edit gives Vite new modules
      const hash = createHash("sha256").update(file).update(profile);
      for (const source of all) hash.update(source).update(readFileSync(source));
      const out = path.join(outBase, hash.digest("hex").slice(0, 16));
      const result = spawnSync(
        binary,
        ["build", file, "--out-dir", out, "--profile", profile, "--color", "never"],
        { cwd: dir, encoding: "utf8" },
      );
      if (result.error !== undefined) {
        this.error(`cannot run ${binary}: ${result.error.message}; set FLICK_BIN or the plugin's \`binary\``);
      }
      if (result.status !== 0) this.error(`flick build ${path.basename(file)} failed:\n${result.stderr}`);
      const name = moduleName(readFileSync(file, "utf8"));
      if (name === undefined) this.error(`${file} has no \`module\` header`);
      const target = path.join(out, `${name.replaceAll(".", "/")}.js`);
      return `export * from ${JSON.stringify(target)};\n`;
    },
  };
};
