// Compiler/worker interoperability only. Widget state and presentation live in Flick.
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

interface Execution {
  readonly lines: Array<Line>;
  readonly status: string;
  readonly detail: string;
  readonly diagnostics: Array<Diagnostic>;
}

let ready: Promise<unknown> | undefined;

/** A rejected initialization is not cached, so Flick's Retry can load again. */
export const load = async (): Promise<string> => {
  try {
    await (ready ??= init());
    return language_version();
  } catch (error) {
    ready = undefined;
    throw error;
  }
};

/** `line:column` (from 1) of a UTF-8 offset of `source`. */
const position = (source: string, offset: number): string => {
  const before = new TextDecoder().decode(new TextEncoder().encode(source).slice(0, offset));
  const lines = before.split("\n");
  return `${lines.length}:${(lines.at(-1)?.length ?? 0) + 1}`;
};

const diagnostics = (source: string, events: ReadonlyArray<Event>): Array<Diagnostic> => events.map((event) => ({
  code: event.code,
  severity: event.severity,
  message: event.message,
  at: event.range === undefined ? "" : position(source, event.range.start),
}));

/** Both real generated profiles, retaining each profile's diagnostics. */
export const compile = (source: string) => {
  const profile = (name: string) => {
    const result = JSON.parse(compileModule(source, name)) as { js: string | null; diagnostics: Array<Event> };
    return { code: result.js ?? "", diagnostics: diagnostics(source, result.diagnostics) };
  };
  return { debug: profile("debug"), release: profile("release") };
};

/** Builds and executes only in the existing isolated worker; abort always terminates it. */
export const run = (source: string, profile: string, signal: AbortSignal): Promise<Execution> => {
  if (signal.aborted) return Promise.resolve({ lines: [], status: "stopped", detail: "", diagnostics: [] });
  const result = JSON.parse(build(source, profile)) as {
    files: Record<string, string> | null;
    entry: string | null;
    diagnostics: Array<Event>;
  };
  const problems = diagnostics(source, result.diagnostics);
  if (result.files === null) return Promise.resolve({ lines: [], status: "invalid", detail: "", diagnostics: problems });
  if (result.entry === null) return Promise.resolve({ lines: [], status: "no-main", detail: "", diagnostics: problems });
  const lines: Array<Line> = [];
  const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
  const { promise, resolve } = Promise.withResolvers<Execution>();
  let settled = false;
  const finish = (status: string, detail: string): void => {
    if (settled) return;
    settled = true;
    signal.removeEventListener("abort", abort);
    worker.terminate();
    resolve({ lines, status, detail, diagnostics: problems });
  };
  const abort = (): void => finish("stopped", "");
  signal.addEventListener("abort", abort, { once: true });
  worker.onmessage = (event: MessageEvent<Message>) => {
    const message = event.data;
    if (message.stream === "exit") finish("exit", String(message.code));
    else if (message.stream === "crash") finish("failure", message.text);
    else lines.push({ stream: message.stream, text: message.text });
  };
  worker.onerror = (event) => finish("failure", event.error instanceof Error ? (event.error.stack ?? event.error.message) : event.message);
  worker.onmessageerror = () => finish("failure", "The worker returned an unreadable message");
  try {
    worker.postMessage({ files: result.files, entry: result.entry });
  } catch (error) {
    finish("failure", error instanceof Error ? (error.stack ?? error.message) : String(error));
  }
  return promise;
};
