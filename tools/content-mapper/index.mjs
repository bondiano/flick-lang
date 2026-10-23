// The Content Mapper adapter for typescript-go, protocol version 1. Flick
// writes a virtual `.flick.ts` projection of each module with span maps
// (`flick inspect <module> --emit mapping`); this adapter serves them to the
// TypeScript 7 API as a virtual file system and maps positions between the
// projection and the `.fl` source. It answers what TypeScript sees of a
// Flick module; Flick's own checker stays the only source of Flick types,
// and the source checker works without this adapter.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { API } from "typescript/unstable/sync";

/** The projection format this adapter reads. */
export const PROTOCOL = 1;

/** The typescript-go release the adapter is verified against. */
export const TYPESCRIPT = "7.0.2";

/** The installed TypeScript; another release is refused, not trusted. */
export const typescriptVersion = () =>
  JSON.parse(readFileSync(new URL(import.meta.resolve("typescript/package.json")), "utf8")).version;

/** The projection of `module` of the project in `cwd`. */
export const project = (flick, cwd, module) => {
  const json = execFileSync(flick, ["inspect", module, "--emit", "mapping"], { cwd, encoding: "utf8" });
  const projected = JSON.parse(json);
  if (projected.version !== PROTOCOL) {
    throw new Error(`projection version ${projected.version}; this adapter reads ${PROTOCOL}`);
  }
  return projected;
};

// Projections count UTF-8 bytes, TypeScript UTF-16 units.
const utf16 = (text, bytes) => Buffer.from(text, "utf8").subarray(0, bytes).toString("utf8").length;
const utf8 = (text, units) => Buffer.byteLength(text.slice(0, units), "utf8");

/** Maps a `.fl` byte offset into the projection through exact mappings. */
export const toProjection = (projected, offset) => {
  const m = projected.mappings.find((m) => m.exact && m.fl[0] <= offset && offset <= m.fl[1]);
  return m === undefined ? undefined : utf16(projected.text, m.ts[0] + (offset - m.fl[0]));
};

/** Maps a projection position back to a `.fl` byte range. */
export const toFlick = (projected, position) => {
  const bytes = utf8(projected.text, position);
  const m = projected.mappings.find((m) => m.ts[0] <= bytes && bytes <= m.ts[1]);
  return m === undefined ? undefined : { start: m.fl[0], end: m.fl[1], exact: m.exact };
};

/**
 * A TypeScript 7 program over the projections, rooted at `root` (a
 * directory whose `node_modules` has `@flick-lang/runtime`).
 */
export const open = (projections, root) => {
  const files = Object.fromEntries(projections.map((p) => [join(root, p.file), p.text]));
  const config = join(root, "tsconfig.flick.json");
  files[config] = JSON.stringify({
    compilerOptions: { strict: true, module: "esnext", moduleResolution: "bundler", target: "esnext", noEmit: true, skipLibCheck: true },
    files: Object.keys(files),
  });
  const fs = {
    // anything but the projections comes from the real file system
    readFile: (name) => files[name],
    fileExists: (name) => (name in files ? true : undefined),
  };
  const api = new API({ fs, cwd: root });
  const snapshot = api.updateSnapshot({ openProjects: [config] });
  const program = snapshot.getProject(config);
  const fileOf = (p) => join(root, p.file);
  return {
    /** What TypeScript sees at a `.fl` offset of `projected`. */
    typeAt(projected, offset) {
      const position = toProjection(projected, offset);
      if (position === undefined) return undefined;
      const type = program.checker.getTypeAtPosition(fileOf(projected), position);
      return type === undefined ? undefined : program.checker.typeToString(type);
    },
    /** TypeScript's diagnostics of a projection, at `.fl` ranges where mapped. */
    diagnostics(projected) {
      return program.program
        .getSemanticDiagnostics(fileOf(projected))
        .map((d) => ({ text: d.text, at: toFlick(projected, d.pos) }));
    },
    close: () => api.close(),
  };
};
