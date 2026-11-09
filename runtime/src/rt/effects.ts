// Tasks as values: a task is a function of nothing that runs when called, and
// these are the functions over them that compiled code and the built-in modules
// name. What a task or a runtime function gives is a value or a call, which the
// driver tells apart, so each of these takes either.
import type { Seq } from "../core/seq.ts";
import { fromNullable } from "../core/data.ts";
import { seq } from "../core/seq.ts";
import { fail, raise } from "./cause.ts";
import { type Ctx, Instruction, type Run, currentCtx, inCtx, isRun } from "./fiber.ts";
import { args, env, writeStderr, writeStdout } from "./host.ts";

function* then(call: unknown, f: (value: unknown) => unknown): Run {
  return f(yield call);
}

/** `f` of what `call` gives: at once when it is a value. */
export const mapped = (call: unknown, f: (value: unknown) => unknown): unknown =>
  isRun(call) || call instanceof Instruction ? then(call, f) : f(call);

/** A foreign `effect` with `catch`: a throw of its factory becomes the mapped error. */
export function* foreignEffect(make: () => unknown, mapping: (error: unknown) => unknown): Run {
  let made: unknown;
  try {
    made = make();
  } catch (error) {
    return raise(fail(mapping(error)));
  }
  return yield made;
}

export const succeed =
  (value: unknown) =>
  (): unknown =>
    value;

export const fromResult = (result: { readonly _tag: string; readonly success?: unknown; readonly failure?: unknown }): unknown =>
  result._tag === "Success" ? result.success : raise(fail(result.failure));

export const bindEff =
  (task: () => unknown, f: (value: unknown) => () => unknown) =>
  (): Run =>
    (function* (): Run {
      return f(yield task())();
    })();

export const flatMapEff =
  (f: (value: unknown) => () => unknown, task: () => unknown) =>
  (): Run =>
    (function* (): Run {
      return f(yield task())();
    })();

export const mapEff =
  (f: (value: unknown) => unknown, task: () => unknown) =>
  (): unknown =>
    mapped(task(), f);

export const map2Eff =
  (f: (a: unknown, b: unknown) => unknown, x: () => unknown, y: () => unknown) =>
  (): Run =>
    (function* (): Run {
      const a: unknown = yield x();
      const b: unknown = yield y();
      return f(a, b);
    })();

export function* listMapEffect(f: (a: unknown) => unknown, xs: ReadonlyArray<unknown>): Run {
  const out: Array<unknown> = [];
  for (const x of xs) out.push(yield f(x));
  return out;
}

export function* listEachEffect(f: (a: unknown) => unknown, xs: ReadonlyArray<unknown>): Run {
  for (const x of xs) yield f(x);
  return undefined;
}

export function* listFoldEffect(f: (acc: unknown, a: unknown) => unknown, init: unknown, xs: ReadonlyArray<unknown>): Run {
  let acc = init;
  for (const x of xs) acc = yield f(acc, x);
  return acc;
}

export function* seqEachEffect(f: (a: unknown) => unknown, xs: Iterable<unknown>): Run {
  for (const x of xs) yield f(x);
  return undefined;
}

export function* seqFoldEffect(f: (acc: unknown, a: unknown) => unknown, init: unknown, xs: Iterable<unknown>): Run {
  let acc = init;
  for (const x of xs) acc = yield f(acc, x);
  return acc;
}

/** The evidence of the stage at hand, for a sequence whose steps read it later. */
export const syncContext = (): Ctx => currentCtx();

/** The sequence of a `for` whose body makes Sync calls: each step runs with `ctx`. */
export const seqIn = <A>(ctx: Ctx, gen: () => Iterator<A>): Seq<A> =>
  seq(function* () {
    const steps = gen();
    try {
      for (;;) {
        const step = inCtx(ctx, () => steps.next());
        if (step.done === true) return;
        yield step.value;
      }
    } finally {
      inCtx(ctx, () => steps.return?.());
    }
  });

export const stdoutLine = (text: string): void => writeStdout(`${text}\n`);
export const stderrLine = (text: string): void => writeStderr(`${text}\n`);
export const stdoutWrite = (text: string): void => writeStdout(text);
export const platformArgs = (): Array<string> => args();
export const platformEnv = (name: string): unknown => fromNullable(env(name));
