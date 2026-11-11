// Compiler/worker interoperability only. Widget state and presentation live in Flick.
import type { Built, Completion, Diagnostic, Generated, Hover, Operation, Reply, Request, Result } from "./compiler.worker.ts";
import { packages } from "./loader.ts";
import type { Message, Program } from "./runtime.ts";
import { notify } from "./dom.ts";

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

/** Declaration graph is requested only for explicit exercise checks. */
export const compileExercise = (source: string, signal: AbortSignal): Promise<Generated> =>
  request<Generated>({ action: "compile", source, profile: "debug", declarations: true }, signal);

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

let disposePreview: (() => void) | undefined;
let previewMessages: Array<Line> = [];
let previewGeneration = 0;

/** Removes the preview and its message listener. */
export const closePreview = (): void => {
  previewGeneration++;
  disposePreview?.();
  disposePreview = undefined;
  previewMessages = [];
  document.getElementById("fp-frame")?.replaceChildren();
};

/** Output sent after a UI program has mounted. */
export const takePreviewMessages = (): Array<Line> => previewMessages.splice(0);

/** A message of the frame, whose program may be anyone's: only the shapes `runtime.ts` posts. */
const fromFrame = (data: unknown): Message | undefined => {
  if (typeof data !== "object" || data === null) return undefined;
  const { stream, text, code } = data as Record<string, unknown>;
  if ((stream === "stdout" || stream === "stderr" || stream === "crash") && typeof text === "string") return { stream, text };
  if (stream === "exit" && typeof code === "number") return { stream, code };
  return stream === "ready" || stream === "mounted" ? { stream } : undefined;
};

/** A frame that allows scripts and nothing else, with a policy that lets it load only the runtime. */
const openFrame = async (generation: number): Promise<HTMLIFrameElement> => {
  const holder = document.getElementById("fp-frame");
  if (holder === null) throw new Error("The playground has no place for a preview");
  const { default: frameScript } = await import("./frame.ts?worker&url");
  if (generation !== previewGeneration) throw new DOMException("The preview was replaced", "AbortError");
  const script = new URL(frameScript, location.href);
  const dark = document.documentElement.dataset.theme !== "light";
  const policy = `default-src 'none'; script-src ${script.origin} blob:; style-src 'unsafe-inline'; img-src data: blob:`;
  const frame = document.createElement("iframe");
  frame.setAttribute("sandbox", "allow-scripts");
  frame.title = holder.dataset.title ?? "";
  frame.srcdoc = `<!doctype html><html lang="${document.documentElement.lang}"><meta charset="utf-8">`
    + `<meta http-equiv="Content-Security-Policy" content="${policy}">`
    + `<style>:root{color-scheme:${dark ? "dark" : "light"};font-family:system-ui,sans-serif}</style>`
    + `<div id="app"></div><script type="module" src="${script.href}"></script></html>`;
  holder.replaceChildren(frame);
  return frame;
};

/**
 * Build off-thread, then execute in the existing isolated, terminable runner: a
 * worker, or for a program with an interface a frame that stays open once it starts.
 */
export const run = async (source: string, profile: string, signal: AbortSignal): Promise<Execution> => {
  closePreview();
  const generation = previewGeneration;
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
  const program: Program = { files: result.files, entry: result.entry };
  const ui = packages(program.files, program.entry).some((spec) =>
    spec.startsWith("@flick-lang/runtime/rt/ui") || spec.startsWith("@flick-lang/effect-ts/ui"));
  const lines: Array<Line> = [];
  const { promise, resolve } = Promise.withResolvers<Execution>();
  let settled = false;
  let close = (): void => { };
  const finish = (status: string, detail: string): void => {
    if (settled) return;
    settled = true;
    signal.removeEventListener("abort", abort);
    if (status !== "preview") close();
    resolve({ lines, status, detail, diagnostics: problems });
  };
  const abort = (): void => finish("stopped", "");
  signal.addEventListener("abort", abort, { once: true });
  const receive = (message: Message, start: () => void): void => {
    if (message.stream === "ready") start();
    else if (message.stream === "mounted" && ui) finish("preview", "0");
    // a program with an interface that has mounted it and not failed is running
    else if (message.stream === "exit") finish(ui && message.code === 0 ? "preview" : "exit", String(message.code));
    else if (message.stream === "crash") finish("failure", message.text);
    else if (message.stream === "stdout" || message.stream === "stderr") lines.push({ stream: message.stream, text: message.text });
  };
  try {
    if (ui) {
      const frame = await openFrame(generation);
      if (settled || signal.aborted) {
        frame.remove();
        return promise;
      }
      const listen = (event: MessageEvent): void => {
        const message = event.source === frame.contentWindow ? fromFrame(event.data) : undefined;
        if (message === undefined) return;
        if (settled) {
          if (message.stream === "stdout" || message.stream === "stderr" || message.stream === "crash") {
            previewMessages.push({ stream: message.stream, text: message.text });
            notify("fp-runtime");
          }
          return;
        }
        receive(message, () => frame.contentWindow?.postMessage(program, "*"));
      };
      close = () => {
        removeEventListener("message", listen);
        if (disposePreview === close) disposePreview = undefined;
        frame.remove();
      };
      disposePreview = close;
      addEventListener("message", listen);
    } else {
      const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
      close = () => worker.terminate();
      worker.onmessage = (event: MessageEvent<Message>) => receive(event.data, () => { });
      worker.onerror = (event) => finish("failure", event.error instanceof Error ? detail(event.error) : event.message);
      worker.onmessageerror = () => finish("failure", "The worker returned an unreadable message");
      worker.postMessage(program);
    }
  } catch (error) {
    finish("failure", detail(error));
  }
  return promise;
};
