// What the Flick module of the playground calls through the FFI: Flick's
// compiler built to WebAssembly, and a worker running a build. Results are
// Flick values: records with the fields of its type aliases, lists as arrays.
import init, { build, compile as compileModule, language_version } from "./wasm/flick_wasm.js";
import type { Message } from "./worker.ts";

interface Diagnostic {
  readonly code: string;
  readonly severity: string;
  readonly message: string;
  readonly at: string;
}

interface Line {
  readonly stream: string;
  readonly text: string;
}

interface Event {
  readonly code: string;
  readonly severity: string;
  readonly message: string;
  readonly range?: { readonly start: number };
}

let ready: Promise<unknown> | undefined;

/** `Playground.load`: the language version, once the compiler is in. */
export const load = async (): Promise<string> => {
  await (ready ??= init());
  return language_version();
};

/** `line:column` (from 1) of a UTF-8 offset of `source`. */
const position = (source: string, offset: number): string => {
  const before = new TextDecoder().decode(new TextEncoder().encode(source).slice(0, offset));
  const lines = before.split("\n");
  return `${lines.length}:${(lines.at(-1)?.length ?? 0) + 1}`;
};

/** `Playground.compile`: both profiles' JS and the diagnostics. */
export const compile = (source: string) => {
  const debug = JSON.parse(compileModule(source, "debug")) as { js: string | null; diagnostics: Array<Event> };
  const release = JSON.parse(compileModule(source, "release")) as { js: string | null };
  const diagnostics: Array<Diagnostic> = debug.diagnostics.map((d) => ({
    code: d.code,
    severity: d.severity,
    message: d.message,
    at: d.range === undefined ? "" : position(source, d.range.start),
  }));
  return { debug: debug.js ?? "", release: release.js ?? "", diagnostics };
};

/** `Playground.run_build`: builds and runs `main` in a worker; its output. */
export const run = (source: string, profile: string, signal: AbortSignal): Promise<Array<Line>> => {
  const result = JSON.parse(build(source, profile)) as { files: Record<string, string> | null; entry: string | null };
  if (result.files === null) return Promise.resolve([{ stream: "status", text: "the module has errors: see Diagnostics" }]);
  if (result.entry === null) return Promise.resolve([{ stream: "status", text: "the module exports no `main`: nothing to run" }]);
  const lines: Array<Line> = [];
  const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
  return new Promise((done) => {
    const finish = (text: string): void => {
      worker.terminate();
      lines.push({ stream: "status", text: `\n— ${text}` });
      done(lines);
    };
    signal.addEventListener("abort", () => finish("stopped"));
    worker.onmessage = (event: MessageEvent<Message>) => {
      const m = event.data;
      if (m.stream === "exit") finish(`exit code ${m.code}`);
      else if (m.stream === "crash") finish(m.text);
      else lines.push({ stream: m.stream, text: m.text });
    };
    worker.postMessage({ files: result.files, entry: result.entry });
  });
};
