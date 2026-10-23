// Runs a build in a worker, so a long or endless program can be stopped:
// `process` is an object whose streams post their text to the page, and
// the runtime's packages are loaded on demand from its built files.
import runtimePackage from "../../../runtime/package.json";
import { type Files, type Namespace, link, packages } from "./loader.ts";

export type Message =
  | { readonly stream: "stdout" | "stderr"; readonly text: string }
  | { readonly stream: "exit"; readonly code: number }
  | { readonly stream: "crash"; readonly text: string };

const post = (message: Message): void => postMessage(message);
const writer = (stream: "stdout" | "stderr") => ({
  write: (text: string): boolean => {
    post({ stream, text });
    return true;
  },
});
const proc = {
  stdout: writer("stdout"),
  stderr: writer("stderr"),
  argv: ["node", "main.js"],
  env: {},
  exitCode: undefined as number | undefined,
  on: (): void => { },
  off: (): void => { },
};
(globalThis as { process?: unknown }).process = proc;

const modules = import.meta.glob<Namespace>([
  "../../../runtime/dist/index.js",
  "../../../runtime/dist/cli/index.js",
  "../../../runtime/dist/effect/*.js",
]);

/** The built file a specifier of the runtime package resolves to. */
const fileOf = (spec: string): string | undefined => {
  const prefix = "@flick-lang/runtime";
  if (spec !== prefix && !spec.startsWith(`${prefix}/`)) return undefined;
  const sub = spec === prefix ? "." : `.${spec.slice(prefix.length)}`;
  const exports = runtimePackage.exports as Record<string, { default: string }>;
  const direct = exports[sub]?.default;
  const pattern = Object.entries(exports).find(([k]) => k.endsWith("*") && sub.startsWith(k.slice(0, -1)));
  const target = direct ?? pattern?.[1].default.replace("*", sub.slice(pattern[0].length - 1));
  return target === undefined ? undefined : `../../../runtime/${target.slice(2)}`;
};

onmessage = async (event: MessageEvent<{ files: Files; entry: string }>) => {
  const { files, entry } = event.data;
  try {
    const store: Record<string, Namespace> = {};
    for (const spec of packages(files, entry)) {
      const load = modules[fileOf(spec) ?? ""];
      if (load === undefined) throw new Error(`the playground cannot load ${spec}`);
      store[spec] = await load();
    }
    (globalThis as { __flickPackages?: unknown }).__flickPackages = store;
    const url = (code: string): string => URL.createObjectURL(new Blob([code], { type: "text/javascript" }));
    await import(/* @vite-ignore */ link(files, entry, url));
    post({ stream: "exit", code: proc.exitCode ?? 0 });
  } catch (error) {
    post({ stream: "crash", text: error instanceof Error ? (error.stack ?? error.message) : String(error) });
  }
};
