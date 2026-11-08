// The compiler's JS glue and WASM are loaded only inside this persistent worker.
import init, { Session, compiler_version } from "./wasm/flick_wasm.js";

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
  /** `line:column` in the module, from the compiler; absent elsewhere. */
  readonly at?: string;
}

interface Emitted {
  readonly js: string | null;
  readonly dts: string | null;
  readonly files: Record<string, string> | null;
  readonly entry: string | null;
  readonly diagnostics: Array<Event>;
}

/** One profile's JavaScript and TypeScript declarations, with its diagnostics. */
export interface Generated {
  readonly code: string;
  readonly dts: string;
  readonly diagnostics: Array<Diagnostic>;
}

export interface Built {
  readonly files: Record<string, string> | null;
  readonly entry: string | null;
  readonly diagnostics: Array<Diagnostic>;
}

export type Operation =
  | { readonly action: "load" }
  | { readonly action: "check"; readonly source: string }
  | { readonly action: "compile"; readonly source: string; readonly profile: string }
  | { readonly action: "build"; readonly source: string; readonly profile: string };

export type Request = Operation & { readonly id: number };
export type Result = string | Array<Diagnostic> | Generated | Built;
export type Reply = { readonly id: number; readonly result: Result } | { readonly error: string };

let ready: Promise<string> | undefined;
// one analysis of each edit serves its check, the emits of either profile and a run
let session: Session | undefined;

const analysed = (source: string): Array<Event> => {
  session ??= new Session();
  return JSON.parse(session.update(source)) as Array<Event>;
};

const diagnostics = (events: ReadonlyArray<Event>): Array<Diagnostic> => events.map((event) => ({
  code: event.code,
  severity: event.severity,
  message: event.message,
  at: event.at ?? "",
}));

const emitted = (source: string, profile: string): Emitted => {
  analysed(source);
  return JSON.parse((session as Session).emit(profile)) as Emitted;
};

const generated = (source: string, profile: string): Generated => {
  const result = emitted(source, profile);
  return { code: result.js ?? "", dts: result.dts ?? "", diagnostics: diagnostics(result.diagnostics) };
};

onmessage = async (event: MessageEvent<Request>) => {
  const request = event.data;
  try {
    const version = await (ready ??= init().then(() => compiler_version()));
    let result: Result;
    switch (request.action) {
      case "load":
        result = version;
        break;
      case "check":
        result = diagnostics(analysed(request.source));
        break;
      case "compile":
        result = generated(request.source, request.profile);
        break;
      case "build": {
        const built = emitted(request.source, request.profile);
        result = { files: built.files, entry: built.entry, diagnostics: diagnostics(built.diagnostics) };
        break;
      }
    }
    postMessage({ id: request.id, result } satisfies Reply);
  } catch (error) {
    postMessage({ error: error instanceof Error ? (error.stack ?? error.message) : String(error) } satisfies Reply);
  }
};

onmessageerror = () => postMessage({ error: "The compiler received an unreadable message" } satisfies Reply);
