// Runs the output of `build` from flick-wasm without a file system: each
// compiled module becomes a module URL whose imports point at the URLs of
// the modules it imports, and each package it imports (the runtime and its
// subpaths) becomes a module re-exporting a namespace loaded by the host.

export type Files = Readonly<Record<string, string>>;

/** A module namespace, as `import()` gives it. */
export type Namespace = Readonly<Record<string, unknown>>;

/** `import … from "x"`, `export … from "x"`, `import "x"` and `import("x")`. */
const SPECIFIER = /(\bfrom\s*|\bimport\s*\(\s*|\bimport\s+)"([^"]+)"/g;

const isRelative = (spec: string): boolean => spec.startsWith("./") || spec.startsWith("../");

/** `spec` imported from `from`, both paths of the output directory. */
export const resolve = (from: string, spec: string): string => {
  const parts = from.split("/").slice(0, -1);
  for (const part of spec.split("/")) {
    if (part === "..") parts.pop();
    else if (part !== ".") parts.push(part);
  }
  return parts.join("/");
};

/** The specifiers `code` imports. */
export const imports = (code: string): Array<string> => [...code.matchAll(SPECIFIER)].map((m) => m[2] ?? "");

/** The packages the modules reachable from `entry` import. */
export const packages = (files: Files, entry: string): Array<string> => {
  const seen = new Set<string>();
  const out = new Set<string>();
  const visit = (path: string): void => {
    if (seen.has(path)) return;
    seen.add(path);
    for (const spec of imports(files[path] ?? "")) {
      if (isRelative(spec)) visit(resolve(path, spec));
      else out.add(spec);
    }
  };
  visit(entry);
  return [...out].sort();
};

/**
 * A module re-exporting the namespace the host stored under `key`. Each
 * name goes through a local, so a reserved word (`Effect.catch`) or any
 * other string is a valid export name.
 */
export const facade = (key: string, namespace: Namespace): string => {
  const names = Object.keys(namespace);
  const lines = [`const m = globalThis.__flickPackages[${JSON.stringify(key)}];`];
  names.forEach((name, i) => lines.push(`const $e${i} = m[${JSON.stringify(name)}];`));
  const exported = names.map((name, i) => `$e${i} as ${JSON.stringify(name)}`);
  if (exported.length > 0) lines.push(`export { ${exported.join(", ")} };`);
  return lines.join("\n");
};

/**
 * The URL of `entry` with every module it reaches linked: `url` turns module
 * text into a URL (a blob URL in a browser, a `data:` URL in Node), and the
 * namespaces of the packages are already in `globalThis.__flickPackages`.
 */
export const link = (files: Files, entry: string, url: (code: string) => string): string => {
  const store = (globalThis as { __flickPackages?: Record<string, Namespace> }).__flickPackages ?? {};
  const linked = new Map<string, string>();
  const external = new Map<string, string>();
  const packageUrl = (spec: string): string => {
    let found = external.get(spec);
    if (found === undefined) {
      const namespace = store[spec];
      if (namespace === undefined) throw new Error(`the playground cannot load ${spec}`);
      found = url(facade(spec, namespace));
      external.set(spec, found);
    }
    return found;
  };
  const visit = (path: string): string => {
    const done = linked.get(path);
    if (done !== undefined) return done;
    const code = files[path];
    if (code === undefined) throw new Error(`the build has no ${path}`);
    const rewritten = code
      .replace(/^[ \t]*\/\/# sourceMappingURL=.*$/gm, "")
      .replace(SPECIFIER, (_, head: string, spec: string) => {
        const target = isRelative(spec) ? visit(resolve(path, spec)) : packageUrl(spec);
        return `${head}${JSON.stringify(target)}`;
      });
    const result = url(rewritten);
    linked.set(path, result);
    return result;
  };
  return visit(entry);
};
