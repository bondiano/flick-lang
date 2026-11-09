// Event handlers and their policies, as values: a `Ui.Handler` is a policy
// and the task of an event. The runtime's bridge runs them.
import type { Binding, DomEvent } from "./nodes.ts";

export type Policy =
  | { readonly kind: "exhaust" }
  | { readonly kind: "latest" }
  | { readonly kind: "queue"; readonly capacity: number }
  | { readonly kind: "concurrent"; readonly limit: number };

/** `Ui.Handler s r e`. */
export interface Handler {
  readonly policy: Policy;
  /** The task of an event: an Effect, or on Flick's own runtime a function of nothing. */
  readonly run: (snapshot: unknown) => unknown;
  /** A pure decision, taken while the browser waits, to prevent the default action. */
  readonly prevent?: (snapshot: unknown) => boolean;
  /** An `Action`: every element that uses it shares one policy state per root. */
  readonly shared?: boolean;
  /** The action's name, `Module.name`, which the compiler gives a top-level one. */
  readonly label?: string;
}

const makeHandler = (policy: Policy, run: Handler["run"]): Handler => Object.freeze({ policy, run });

const positive = (what: string, n: number): number => {
  if (!Number.isInteger(n) || n <= 0) {
    throw new RangeError(`the ${what} of an event policy must be a positive integer, not ${String(n)}`);
  }
  return n;
};

export const eventExhaust = (run: Handler["run"]): Handler => makeHandler({ kind: "exhaust" }, run);
export const eventLatest = (run: Handler["run"]): Handler => makeHandler({ kind: "latest" }, run);
export const eventQueue = (capacity: number, run: Handler["run"]): Handler =>
  makeHandler({ kind: "queue", capacity: positive("capacity", capacity) }, run);
export const eventConcurrent = (limit: number, run: Handler["run"]): Handler =>
  makeHandler({ kind: "concurrent", limit: positive("limit", limit) }, run);
export const eventPreventDefault = (decide: (snapshot: unknown) => boolean, inner: Handler): Handler =>
  Object.freeze({ ...inner, prevent: decide });

/** The actions the setup of the instance at hand makes, while it runs. */
let made: Array<Handler> | undefined;

const makeAction = (policy: Policy, run: Handler["run"]): Handler => {
  const action: Handler = Object.freeze({ policy, run, shared: true });
  made?.push(action);
  return action;
};

/** What `setup` returns, and the actions it made: those of an instance. */
export const instanceActions = <A>(setup: () => A): readonly [A, ReadonlyArray<Handler>] => {
  const saved = made;
  const actions: Array<Handler> = [];
  made = actions;
  try {
    return [setup(), actions];
  } finally {
    made = saved;
  }
};

export const actionExhaust = (run: Handler["run"]): Handler => makeAction({ kind: "exhaust" }, run);
export const actionLatest = (run: Handler["run"]): Handler => makeAction({ kind: "latest" }, run);
export const actionQueue = (capacity: number, run: Handler["run"]): Handler =>
  makeAction({ kind: "queue", capacity: positive("capacity", capacity) }, run);
export const actionConcurrent = (limit: number, run: Handler["run"]): Handler =>
  makeAction({ kind: "concurrent", limit: positive("limit", limit) }, run);

/**
 * `Action.with action arg`: the action given `arg` with each snapshot. Each
 * element that uses it has a policy state of its own, as an editor of one
 * item of a list does.
 */
export const actionWith = (action: Handler, arg: unknown): Handler => {
  const { prevent, label } = action;
  return Object.freeze({
    policy: action.policy,
    run: (snapshot: unknown) => action.run([arg, snapshot]),
    ...(label === undefined ? {} : { label }),
    ...(prevent === undefined ? {} : { prevent: (snapshot: unknown) => prevent([arg, snapshot]) }),
  });
};

export interface Listener {
  readonly callback: (event: DomEvent) => void;
  update(binding: Binding): void;
  close(): void;
}
