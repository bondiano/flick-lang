// The compiler's JS glue and WASM are loaded only inside this persistent worker.
import init, { Session, compiler_version, explain, format } from "./wasm/flick_wasm.js";

/** A change a fix makes, in UTF-16 offsets of the module. */
export interface Edit {
  readonly start: number;
  readonly end: number;
  readonly insert: string;
}

export interface Fix {
  readonly message: string;
  readonly edits: Array<Edit>;
}

/** `start` and `end` are UTF-16 offsets of the module, or -1 for a place elsewhere. */
export interface Diagnostic {
  readonly code: string;
  readonly severity: string;
  readonly message: string;
  readonly at: string;
  readonly start: number;
  readonly end: number;
  readonly fixes: Array<Fix>;
}

interface Located {
  readonly from?: number;
  readonly to?: number;
}

interface Event extends Located {
  readonly code: string;
  readonly severity: string;
  readonly message: string;
  /** `line:column` in the module, from the compiler; absent elsewhere. */
  readonly at?: string;
  readonly fixes?: Array<{ readonly message: string; readonly edits: Array<Located & { readonly replacement: string }> }>;
}

export interface Hover {
  readonly text: string;
  readonly from: number;
  readonly to: number;
}

export interface Completion {
  readonly label: string;
  readonly kind: string;
  readonly detail: string | null;
}

interface Emitted {
  readonly js: string | null;
  readonly dts: string | null;
  readonly declarations?: Record<string, string>;
  readonly declaration_entry?: string;
  readonly files: Record<string, string> | null;
  readonly entry: string | null;
  readonly diagnostics: Array<Event>;
}

/** One profile's JavaScript and TypeScript declarations, with its diagnostics. */
export interface Generated {
  readonly code: string;
  readonly dts: string;
  readonly declarations?: Readonly<Record<string, string>>;
  readonly declarationEntry?: string;
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
  | { readonly action: "compile"; readonly source: string; readonly profile: string; readonly declarations?: boolean }
  | { readonly action: "build"; readonly source: string; readonly profile: string }
  | { readonly action: "hover"; readonly source: string; readonly at: number }
  | { readonly action: "complete"; readonly source: string; readonly at: number; readonly dotted: boolean }
  | { readonly action: "format"; readonly source: string }
  | { readonly action: "explain"; readonly code: string };

export type Request = Operation & { readonly id: number };
export type Result = string | Array<Diagnostic> | Generated | Built | Hover | null | Array<Completion> | { readonly text?: string };
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
  start: event.from ?? -1,
  end: event.to ?? -1,
  // a fix that edits a built-in module cannot apply to the editor
  fixes: (event.fixes ?? [])
    .filter((fix) => fix.edits.every((edit) => edit.from !== undefined))
    .map((fix) => ({
      message: fix.message,
      edits: fix.edits.map((edit) => ({ start: edit.from ?? 0, end: edit.to ?? 0, insert: edit.replacement })),
    })),
}));

const emitted = (source: string, profile: string, declarations = false): Emitted => {
  analysed(source);
  const current = session as Session;
  return JSON.parse(declarations ? current.emit_declarations(profile) : current.emit(profile)) as Emitted;
};

const generated = (source: string, profile: string, declarations = false): Generated => {
  const result = emitted(source, profile, declarations);
  return { code: result.js ?? "", dts: result.dts ?? "", declarations: result.declarations, declarationEntry: result.declaration_entry, diagnostics: diagnostics(result.diagnostics) };
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
        result = generated(request.source, request.profile, request.declarations);
        break;
      case "build": {
        const built = emitted(request.source, request.profile);
        result = { files: built.files, entry: built.entry, diagnostics: diagnostics(built.diagnostics) };
        break;
      }
      case "hover":
        analysed(request.source);
        result = JSON.parse((session as Session).hover(request.at)) as Hover | null;
        break;
      case "complete":
        analysed(request.source);
        result = JSON.parse((session as Session).complete(request.at, request.dotted)) as Array<Completion>;
        break;
      case "format":
        result = JSON.parse(format(request.source)) as { readonly text?: string };
        break;
      case "explain":
        result = explain(request.code);
        break;
    }
    postMessage({ id: request.id, result } satisfies Reply);
  } catch (error) {
    postMessage({ error: error instanceof Error ? (error.stack ?? error.message) : String(error) } satisfies Reply);
  }
};

onmessageerror = () => postMessage({ error: "The compiler received an unreadable message" } satisfies Reply);
