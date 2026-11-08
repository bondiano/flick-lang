// Compiler/worker interoperability only. Widget state and presentation live in Flick.
import type { Built, Completion, Diagnostic, Generated, Hover, Operation, Reply, Request, Result } from "./compiler.worker.ts";
import type { Message } from "./worker.ts";

interface Line {
  readonly stream: string;
  readonly text: string;
}

interface Pending {
  readonly message: Request;
  readonly resolve: (result: Result) => void;
  readonly reject: (error: unknown) => void;
}
interface Execution {
  readonly lines: Array<Line>;
  readonly status: string;
  readonly detail: string;
  readonly diagnostics: Array<Diagnostic>;
}

let compiler: Worker | undefined;
let ready: Promise<string> | undefined;
let sequence = 0;
let active: number | undefined;
const pending = new Map<number, Pending>();

const detail = (error: unknown): string => error instanceof Error ? (error.stack ?? error.message) : String(error);

/** Discard a failed instance so the next explicit request can initialize afresh. */
const fail = (worker: Worker, error: Error): void => {
  if (compiler !== worker) return;
  compiler = undefined;
  ready = undefined;
  active = undefined;
  worker.terminate();
  for (const call of pending.values()) call.reject(error);
};

/** Backpressure keeps canceled queued work out of the worker entirely. */
const dispatch = (worker: Worker): void => {
  if (active !== undefined) return;
  const next = pending.entries().next().value;
  if (next === undefined) return;
  const [id, call] = next;
  active = id;
  try {
    worker.postMessage(call.message);
  } catch (error) {
    fail(worker, new Error(detail(error)));
  }
};

const instance = (): Worker => {
  if (compiler !== undefined) return compiler;
  const worker = new Worker(new URL("./compiler.worker.ts", import.meta.url), { type: "module" });
  compiler = worker;
  worker.onmessage = (event: MessageEvent<Reply>) => {
    if (compiler !== worker) return;
    const reply = event.data;
    if ("error" in reply) fail(worker, new Error(reply.error));
    else {
      if (active === reply.id) active = undefined;
      pending.get(reply.id)?.resolve(reply.result);
      dispatch(worker);
    }
  };
  worker.onerror = (event) => fail(worker, new Error(event.error instanceof Error ? detail(event.error) : event.message));
  worker.onmessageerror = () => fail(worker, new Error("The compiler returned an unreadable message"));
  return worker;
};

const request = <T>(operation: Operation, signal?: AbortSignal): Promise<T> => {
  if (signal?.aborted) return Promise.reject(signal.reason ?? new DOMException("The operation was aborted", "AbortError"));
  let worker: Worker;
  try {
    worker = instance();
  } catch (error) {
    return Promise.reject(error);
  }
  const { promise, resolve, reject } = Promise.withResolvers<T>();
  const id = ++sequence;
  const cleanup = (): void => {
    pending.delete(id);
    signal?.removeEventListener("abort", abort);
  };
  // shortcut: active WASM cannot be interrupted; reload the shared compiler if hard cancellation becomes necessary.
  const abort = (): void => {
    cleanup();
    reject(signal?.reason ?? new DOMException("The operation was aborted", "AbortError"));
  };
  pending.set(id, {
    message: { ...operation, id },
    resolve: (result) => { cleanup(); resolve(result as T); },
    reject: (error) => { cleanup(); reject(error); },
  });
  signal?.addEventListener("abort", abort, { once: true });
  dispatch(worker);
  return promise;
};

/** Initialization and its language version are shared; a failure permits Retry. */
export const load = async (): Promise<string> => {
  const loading = ready ??= request<string>({ action: "load" });
  try {
    return await loading;
  } catch (error) {
    if (ready === loading) ready = undefined;
    throw error;
  }
};

/** The diagnostics of `source`: an analysis, which is quick, and no emit. */
export const check = (source: string, signal: AbortSignal): Promise<Array<Diagnostic>> =>
  request<Array<Diagnostic>>({ action: "check", source }, signal);

/** What the compiler writes for `source` in `profile`: its JavaScript, `.d.ts` and diagnostics. */
export const compile = (source: string, profile: string, signal: AbortSignal): Promise<Generated> =>
  request<Generated>({ action: "compile", source, profile }, signal);

/**
 * Markdown as plain text: a code block keeps its lines without its fences,
 * and the lines of a paragraph, which the compiler wraps, join.
 */
export const plain = (markdown: string): string =>
  markdown
    .split(/^```[^\n]*\n?/m)
    .map((part, i) => (i % 2 === 1 ? part : part.replace(/([^\n])\n(?![\n\s>#*-]|\d+\.)/g, "$1 ")))
    .join("")
    .trim();

/** What hovering at the UTF-16 offset `at` of `source` shows, as plain text, or null. */
export const hover = async (source: string, at: number, signal?: AbortSignal): Promise<Hover | null> => {
  const shown = await request<Hover | null>({ action: "hover", source, at }, signal);
  return shown === null ? null : { ...shown, text: plain(shown.text) };
};

/** What may stand at the UTF-16 offset `at` of `source`: the start of a name, or with `dotted` a `.` whose member is cut out of `source`. */
export const complete = (source: string, at: number, dotted: boolean, signal?: AbortSignal): Promise<Array<Completion>> =>
  request<Array<Completion>>({ action: "complete", source, at, dotted }, signal);

/** `source` formatted, or the same text when it does not parse. */
export const formatSource = async (source: string): Promise<string> =>
  (await request<{ readonly text?: string }>({ action: "format", source })).text ?? source;

/** What the diagnostic `code` means, as plain text; empty for a code the compiler does not have. */
export const explainCode = async (code: string): Promise<string> => plain(await request<string>({ action: "explain", code }));

/** Build off-thread, then execute in the existing isolated, terminable runner. */
export const run = async (source: string, profile: string, signal: AbortSignal): Promise<Execution> => {
  if (signal.aborted) return { lines: [], status: "stopped", detail: "", diagnostics: [] };
  let result: Built;
  try {
    result = await request<Built>({ action: "build", source, profile }, signal);
  } catch (error) {
    if (signal.aborted) return { lines: [], status: "stopped", detail: "", diagnostics: [] };
    throw error;
  }
  const problems = result.diagnostics;
  if (signal.aborted) return { lines: [], status: "stopped", detail: "", diagnostics: problems };
  if (result.files === null) return { lines: [], status: "invalid", detail: "", diagnostics: problems };
  if (result.entry === null) return { lines: [], status: "no-main", detail: "", diagnostics: problems };
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
  worker.onerror = (event) => finish("failure", event.error instanceof Error ? detail(event.error) : event.message);
  worker.onmessageerror = () => finish("failure", "The worker returned an unreadable message");
  try {
    worker.postMessage({ files: result.files, entry: result.entry });
  } catch (error) {
    finish("failure", detail(error));
  }
  return promise;
};
