// What a build runs on in the playground, in a worker or in the preview's frame:
// `process` is an object whose streams post their text to the page, and
// the runtime's packages are loaded on demand from their built files: Flick's
// own runtime, and the Effect adapter for the bindings it does not have yet.
import runtimePackage from "../../../runtime/package.json" with { type: "json" };
import effectPackage from "../../../packages/effect-ts/package.json" with { type: "json" };
import { type Files, type Namespace, link, packages } from "./loader.ts";

export type Message =
  | { readonly stream: "stdout" | "stderr"; readonly text: string }
  | { readonly stream: "exit"; readonly code: number }
  | { readonly stream: "crash"; readonly text: string }
  /** The frame has loaded and waits for the program. */
  | { readonly stream: "ready" | "mounted" };

export interface Program {
  readonly files: Files;
  readonly entry: string;
  readonly tests?: { readonly module: string; readonly options: BrowserTestOptions };
}

export interface BrowserTestOptions {
  readonly list?: boolean;
  readonly filter?: string;
  readonly timeout?: number;
  readonly jobs?: number;
  readonly seed?: number;
  readonly forbidOnly?: boolean;
  readonly reporter?: "human" | "json";
}

const modules = import.meta.glob<Namespace>([
  "../../../runtime/dist/**/*.js",
  "../../../packages/effect-ts/dist/index.js",
  "../../../packages/effect-ts/dist/bridge.js",
  "../../../packages/effect-ts/dist/cli/index.js",
  "../../../packages/effect-ts/dist/ui/index.js",
  "../../../packages/effect-ts/dist/ui/react/index.js",
  "../../../packages/effect-ts/dist/effect/*.js",
  "../../../packages/effect-ts/dist/effect/http/*.js",
  // These adapters (including Response through Multipart) load Node services.
  "!../../../packages/effect-ts/dist/effect/http/{HttpServer,HttpPlatform,HttpMiddleware,HttpStaticServer,HttpServerResponse,Multipart}.js",
  "../../../packages/effect-ts/dist/effect/reactivity/*.js",
  "../../../packages/effect-ts/dist/effect/observability/*.js",
  "../../../packages/effect-ts/dist/effect/devtools/DevTools.js",
  "../../../packages/effect-ts/dist/effect/rpc/Rpc.js",
]);

/** The built file a specifier of the runtime package resolves to. */
const fileOf = (spec: string): string | undefined => {
  const effect = spec === "@flick-lang/effect-ts" || spec.startsWith("@flick-lang/effect-ts/");
  const prefix = effect ? "@flick-lang/effect-ts" : "@flick-lang/runtime";
  if (spec !== prefix && !spec.startsWith(`${prefix}/`)) return undefined;
  const sub = spec === prefix ? "." : `.${spec.slice(prefix.length)}`;
  const exports = (effect ? effectPackage.exports : runtimePackage.exports) as Record<string, { default: string }>;
  const direct = exports[sub]?.default;
  const pattern = Object.entries(exports).find(([k]) => k.endsWith("*") && sub.startsWith(k.slice(0, -1)));
  const target = direct ?? pattern?.[1].default.replace("*", sub.slice(pattern[0].length - 1));
  return target === undefined ? undefined : `../../../${effect ? "packages/effect-ts" : "runtime"}/${target.slice(2)}`;
};

/** Runs the program, posting what it writes, then `exit` with its code or `crash`. */
export const execute = async (program: Program, post: (message: Message) => void, signal?: AbortSignal): Promise<void> => {
  let { files, entry } = program;
  if (program.tests !== undefined) {
    const launcher = "flick-tests.js";
    files = { ...files, [launcher]: `import { runTests } from "@flick-lang/runtime/rt/testing";\nimport * as suite from ${JSON.stringify(`./${entry}`)};\nawait runTests([[${JSON.stringify(program.tests.module)}, suite.tests]], ${JSON.stringify(program.tests.options)});\n` };
    entry = launcher;
  }
  const events = new EventTarget();
  const listeners = new Map<() => void, EventListener>();
  const writer = (stream: "stdout" | "stderr") => ({
    write: (text: string): boolean => {
      post({ stream, text });
      return true;
    },
    on: (): void => { },
  });
  const proc = {
    stdout: writer("stdout"),
    stderr: writer("stderr"),
    argv: ["node", "flick-main.js"],
    env: {},
    exitCode: undefined as number | undefined,
    on: (): void => { },
    once: (name: string, listener: () => void): void => {
      const wrapped: EventListener = () => listener();
      listeners.set(listener, wrapped);
      events.addEventListener(name, wrapped, { once: true });
      if (name === "SIGINT" && signal?.aborted) wrapped(new Event(name));
    },
    off: (name: string, listener: () => void): void => {
      const wrapped = listeners.get(listener);
      if (wrapped !== undefined) events.removeEventListener(name, wrapped);
      listeners.delete(listener);
    },
  };
  const stop = (): void => { events.dispatchEvent(new Event("SIGINT")); };
  signal?.addEventListener("abort", stop, { once: true });
  (globalThis as { process?: unknown }).process = proc;
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
  } finally {
    signal?.removeEventListener("abort", stop);
  }
};
