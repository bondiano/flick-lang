// Builds `.fl` imports with the native compiler; Vite resolves the emitted
// runtime, built-ins and foreign JavaScript imports as usual.
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import path from "node:path";
import { promisify } from "node:util";

const compile = promisify(execFile);

/**
 * @param {{ binary?: string, profile?: "debug" | "release", outDir?: string }} options
 * `binary` is the `flick` CLI (`FLICK_BIN`, else `flick` on the PATH);
 * `profile` is by default `release` for `vite build` and `debug` otherwise;
 * `outDir` holds the builds (by default `<vite cache>/flick`).
 */
export const flickVite = (options = {}) => {
  const binary = options.binary ?? process.env.FLICK_BIN ?? "flick";
  let profile = options.profile ?? "debug";
  let outBase = options.outDir;
  let watcher;
  const builds = new Map();
  const watched = new Map();
  return {
    name: "flick",
    enforce: "pre",
    configResolved(config) {
      outBase ??= path.join(config.cacheDir, "flick");
      if (options.profile === undefined && config.command === "build") profile = "release";
    },
    configureServer(server) {
      watcher = server.watcher;
    },
    async load(id) {
      const file = path.resolve(id.split("?")[0]);
      if (!file.endsWith(".fl")) return null;
      const key = JSON.stringify([file, profile, outBase]);
      // Only concurrent SSR/client transforms share a build; the CLI owns caching.
      if (!builds.has(key)) {
        const inputs = compile(
          binary,
          ["build", file, "--inputs", "--color", "never"],
          { cwd: path.dirname(file), encoding: "utf8" },
        ).then(({ stdout }) => JSON.parse(stdout));
        const output = inputs.then(async (inventory) => {
          const name = inventory.modules.find((module) => path.resolve(module.path) === file)?.name;
          if (!name) throw new Error(`${file} has no \`module\` header`);
          const hash = createHash("sha256").update(JSON.stringify([file, profile, inventory.key]));
          const out = path.join(outBase, hash.digest("hex").slice(0, 16));
          await compile(
            binary,
            ["build", file, "--emit", "js", "--out-dir", out, "--profile", profile, "--color", "never"],
            { cwd: path.dirname(file), encoding: "utf8" },
          );
          return path.join(out, `${name.replaceAll(".", "/")}.js`);
        }).finally(() => builds.delete(key));
        builds.set(key, { inputs, output });
      }
      const pending = builds.get(key);
      try {
        const [, target] = await Promise.all([
          pending.inputs.then((inventory) => {
            const files = inventory.files.map((source) => path.resolve(source));
            const roots = inventory.roots.map((root) => path.resolve(root));
            watched.set(file, { files: new Set(files), roots });
            // Vite treats addWatchFile paths as imports in dev, unlike its watcher.
            const paths = [...files, ...roots];
            if (watcher) watcher.add(paths);
            else for (const source of paths) this.addWatchFile(source);
          }),
          pending.output,
        ]);
        return `export * from ${JSON.stringify(target)};\n`;
      } catch (error) {
        if (typeof error.code === "string") {
          this.error(`cannot run ${binary}: ${error.message}; set FLICK_BIN or the plugin's \`binary\``);
        }
        this.error(`flick build ${path.basename(file)} failed:\n${error.stderr ?? error.message}`);
      }
    },
    hotUpdate({ file, modules, timestamp }) {
      const changed = path.resolve(file);
      const affected = new Set(modules);
      const graph = this.environment.moduleGraph;
      const invalidated = new Set();
      for (const [source, inputs] of watched) {
        const inRoot = changed.endsWith(".fl") && inputs.roots.some((root) =>
          changed.startsWith(root.endsWith(path.sep) ? root : `${root}${path.sep}`));
        if (!inputs.files.has(changed) && !inRoot) continue;
        for (const module of graph.getModulesByFile(source) ?? []) {
          affected.add(module);
          graph.invalidateModule(module, invalidated, timestamp, true);
        }
      }
      // Native inputs aren't runtime modules, so runners cannot trace their reload trigger.
      if (invalidated.size > 0) {
        const cache = this.environment.runner?.evaluatedModules;
        if (cache) {
          for (const module of invalidated) {
            const evaluated = module.id && cache.getModuleById(module.id);
            if (evaluated) cache.invalidateModule(evaluated);
          }
        }
      }
      return [...affected];
    },
  };
};
