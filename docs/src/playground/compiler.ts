// Compiler/worker interoperability only. Widget state and presentation live in Flick.
import type { Built, Compiled, Diagnostic, Operation, Reply, Request, Result } from "./compiler.worker.ts";
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

/** Both real profiles, with each profile's generated code and diagnostics. */
export const compile = (source: string, signal: AbortSignal): Promise<Compiled> =>
  request<Compiled>({ action: "compile", source }, signal);

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
