#!/usr/bin/env node
// Takes a snapshot of the public surface of an installed Effect for the
// coverage linter: every entrypoint of its package.json, and for the
// entrypoints the standard library wraps, every export of every module as
// `Module.name` with its kind and a fingerprint of its declarations.
// Usage: surface.mjs [<effect package dir>] > stdlib/Effect/snapshot.json
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import ts from "typescript";

/** The entrypoints whose modules the standard library wraps export by export. */
export const WRAPPED = ["effect", "effect/cli", "effect/http", "effect/http-api"];

const printer = ts.createPrinter({ removeComments: true, newLine: ts.NewLineKind.LineFeed });

/** What a symbol is: `value`, `type` or `value+type`. */
const kindOf = (symbol) => {
  const value = (symbol.flags & ts.SymbolFlags.Value) !== 0;
  const type = (symbol.flags & (ts.SymbolFlags.Type | ts.SymbolFlags.Namespace)) !== 0;
  return value && type ? "value+type" : value ? "value" : "type";
};

/** 16 hex digits of the SHA-256 of the declarations as printed without comments. */
const fingerprint = (symbol) => {
  const text = (symbol.declarations ?? [])
    .map((d) => printer.printNode(ts.EmitHint.Unspecified, d, d.getSourceFile()).replace(/\s+/g, " "))
    .join("\n");
  return createHash("sha256").update(text).digest("hex").slice(0, 16);
};

const program = (file) =>
  ts.createProgram([file], {
    module: ts.ModuleKind.NodeNext,
    moduleResolution: ts.ModuleResolutionKind.NodeNext,
    target: ts.ScriptTarget.ESNext,
    strict: true,
    noEmit: true,
    skipLibCheck: true,
  });

/** The snapshot of the Effect installed at `dir`. */
export const snapshot = (dir) => {
  const manifest = JSON.parse(readFileSync(path.join(dir, "package.json"), "utf8"));
  const entrypoints = Object.entries(manifest.exports ?? {})
    .filter(([key, target]) => target !== null && key !== "./package.json" && !key.includes("*"))
    .map(([key]) => (key === "." ? manifest.name : `${manifest.name}/${key.slice(2)}`))
    .sort();
  const modules = {};
  for (const entrypoint of WRAPPED) {
    if (!entrypoints.includes(entrypoint)) continue;
    const sub = entrypoint === manifest.name ? "." : `./${entrypoint.slice(manifest.name.length + 1)}`;
    const target = manifest.exports[sub];
    const js = typeof target === "string" ? target : (target.types ?? target.default);
    const dts = path.join(dir, js.replace(/\.js$/, ".d.ts"));
    modules[entrypoint] = exportsOf(dts, entrypoint);
  }
  return { package: manifest.name, version: manifest.version, entrypoints, modules };
};

/** `Module.name` → `kind fingerprint` for every export of the entrypoint's modules. */
const exportsOf = (file, entrypoint) => {
  const p = program(file);
  const checker = p.getTypeChecker();
  const root = checker.getSymbolAtLocation(p.getSourceFile(file));
  const resolve = (s) => (s.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(s) : s);
  const out = [];
  for (const exported of checker.getExportsOfModule(root)) {
    const symbol = resolve(exported);
    const isModule = (symbol.flags & ts.SymbolFlags.ValueModule) !== 0 && symbol.declarations?.some(ts.isSourceFile);
    if (!isModule) {
      out.push([`${entrypoint}.${exported.name}`, `${kindOf(symbol)} ${fingerprint(symbol)}`]);
      continue;
    }
    for (const member of checker.getExportsOfModule(symbol)) {
      const target = resolve(member);
      out.push([`${exported.name}.${member.name}`, `${kindOf(target)} ${fingerprint(target)}`]);
    }
  }
  return Object.fromEntries(out.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
};

/** The snapshot as JSON, one export per line. */
export const render = (snap) => {
  const lines = ["{", `  "package": ${JSON.stringify(snap.package)},`, `  "version": ${JSON.stringify(snap.version)},`];
  lines.push(`  "entrypoints": [`);
  lines.push(snap.entrypoints.map((e) => `    ${JSON.stringify(e)}`).join(",\n"));
  lines.push(`  ],`, `  "modules": {`);
  const modules = Object.entries(snap.modules).map(([entry, exports]) => {
    const body = Object.entries(exports).map(([k, v]) => `      ${JSON.stringify(k)}: ${JSON.stringify(v)}`);
    return `    ${JSON.stringify(entry)}: {\n${body.join(",\n")}\n    }`;
  });
  lines.push(modules.join(",\n"), "  }", "}");
  return `${lines.join("\n")}\n`;
};

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const here = path.dirname(fileURLToPath(import.meta.url));
  const dir = process.argv[2] ?? path.join(here, "../../runtime/node_modules/effect");
  process.stdout.write(render(snapshot(dir)));
}
