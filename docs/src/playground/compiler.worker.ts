// The compiler's JS glue and WASM are loaded only inside this persistent worker.
import init, { build, compile as compileModule, language_version } from "./wasm/flick_wasm.js";

export interface Diagnostic {
  readonly code: string;
  readonly severity: string;
  readonly message: string;
  readonly at: string;
}

interface Event {
  readonly code: string;
  readonly severity: string;
  readonly message: string;
  readonly range?: { readonly start: number };
}

export interface Compiled {
  readonly debug: { readonly code: string; readonly diagnostics: Array<Diagnostic> };
  readonly release: { readonly code: string; readonly diagnostics: Array<Diagnostic> };
}

export interface Built {
  readonly files: Record<string, string> | null;
  readonly entry: string | null;
  readonly diagnostics: Array<Diagnostic>;
}

export type Operation =
  | { readonly action: "load" }
  | { readonly action: "compile"; readonly source: string }
  | { readonly action: "build"; readonly source: string; readonly profile: string };

export type Request = Operation & { readonly id: number };
export type Result = string | Compiled | Built;
export type Reply = { readonly id: number; readonly result: Result } | { readonly error: string };

let ready: Promise<string> | undefined;

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

const generated = (source: string, profile: string) => {
  const result = JSON.parse(compileModule(source, profile)) as { js: string | null; diagnostics: Array<Event> };
  return { code: result.js ?? "", diagnostics: diagnostics(source, result.diagnostics) };
};

onmessage = async (event: MessageEvent<Request>) => {
  const request = event.data;
  try {
    const version = await (ready ??= init().then(() => language_version()));
    let result: Result;
    switch (request.action) {
      case "load":
        result = version;
        break;
      case "compile":
        result = { debug: generated(request.source, "debug"), release: generated(request.source, "release") };
        break;
      case "build": {
        const built = JSON.parse(build(request.source, request.profile)) as {
          files: Record<string, string> | null;
          entry: string | null;
          diagnostics: Array<Event>;
        };
        result = { files: built.files, entry: built.entry, diagnostics: diagnostics(request.source, built.diagnostics) };
        break;
      }
    }
    postMessage({ id: request.id, result } satisfies Reply);
  } catch (error) {
    postMessage({ error: error instanceof Error ? (error.stack ?? error.message) : String(error) } satisfies Reply);
  }
};

onmessageerror = () => postMessage({ error: "The compiler received an unreadable message" } satisfies Reply);
