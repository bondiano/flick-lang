// The playground's editor features, in a worker of their own so that a type
// error or an edit under the cursor never waits for a build.
import type { Action, Hint, Signature, Token, Tokens } from "./ide-worker.ts";

export type Operation =
  | { readonly action: "tokens"; readonly source: string }
  | { readonly action: "hints"; readonly source: string }
  | { readonly action: "signature"; readonly source: string; readonly at: number }
  | { readonly action: "actions"; readonly source: string; readonly from: number; readonly to: number };

export type Request = Operation & { readonly id: number };
export type Result = Tokens | ReadonlyArray<Hint> | Signature | ReadonlyArray<Action> | null;
export type Reply = { readonly id: number; readonly result: Result } | { readonly id: number; readonly error: string };

interface Pending {
  readonly resolve: (result: Result) => void;
  readonly reject: (error: unknown) => void;
}

let worker: Worker | undefined;
let sequence = 0;
const pending = new Map<number, Pending>();

const instance = (): Worker => {
  if (worker !== undefined) return worker;
  const created = new Worker(new URL("./ide.worker.ts", import.meta.url), { type: "module" });
  created.onmessage = (event: MessageEvent<Reply>) => {
    const call = pending.get(event.data.id);
    if (call === undefined) return;
    pending.delete(event.data.id);
    if ("error" in event.data) call.reject(new Error(event.data.error));
    else call.resolve(event.data.result);
  };
  created.onerror = (event) => {
    created.terminate();
    if (worker === created) worker = undefined;
    for (const call of pending.values()) call.reject(event);
    pending.clear();
  };
  worker = created;
  return created;
};

/** Asks the editor worker for `operation`; rejects when the worker fails or `signal` aborts. */
export const request = (operation: Operation, signal?: AbortSignal): Promise<Result> => {
  const id = sequence++;
  const message: Request = { ...operation, id };
  return new Promise<Result>((resolve, reject) => {
    if (signal?.aborted === true) {
      reject(new Error("aborted"));
      return;
    }
    signal?.addEventListener("abort", () => {
      pending.delete(id);
      reject(new Error("aborted"));
    });
    pending.set(id, { resolve, reject });
    try {
      instance().postMessage(message);
    } catch (error) {
      pending.delete(id);
      reject(error);
    }
  });
};
