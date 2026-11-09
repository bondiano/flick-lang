// Sync stages: a function whose services are `Atom.Read` alone, with no
// errors, runs at once instead of building an Effect. Its operations read
// the services of the context set around it: by `syncIn` where the runtime
// calls it (a render, a derived atom), by `sync` inside a fiber.
import { Context, Effect } from "effect";

import { type Seq, seq } from "./core/seq.ts";
import { implementation } from "./effects.ts";

/** No Sync stage is running: nothing set the services it reads. */
const OUTSIDE: Context.Context<never> = /*#__PURE__*/ Context.empty();

let current: Context.Context<never> = OUTSIDE;

/** Calls `f` with `context` as the services of its Sync calls. */
export const syncIn = <A>(context: Context.Context<never>, f: () => A): A => {
  const saved = current;
  current = context;
  try {
    return f();
  } finally {
    current = saved;
  }
};

/** A Sync call inside a fiber: it reads the fiber's services. */
export const sync = <A>(f: () => A): Effect.Effect<A> =>
  Effect.contextWith((context: Context.Context<never>) => Effect.sync(() => syncIn(context, f)));

/**
 * An exported function whose last stage, after `stages` arguments, is Sync.
 * A Flick caller is inside a Sync stage or `sync` and gets the value at
 * once, as before; a caller from TypeScript is outside one and gets an
 * `Effect` that reads the fiber's services, as its declared type says.
 */
export const syncExport = (f: unknown, stages: number): unknown => (arg: unknown) => {
  if (typeof f !== "function") throw new TypeError("a Sync export is a function");
  if (stages > 1) return syncExport(Reflect.apply(f, undefined, [arg]), stages - 1);
  const call = (): unknown => Reflect.apply(f, undefined, [arg]);
  return current === OUTSIDE ? sync(call) : call();
};

/** A runtime call in a Sync stage: runs at once with the context's services. */
export const runIn = <A>(effect: Effect.Effect<A, unknown, unknown>): A =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker allows only the stage's services
  Effect.runSyncWith(current)(effect as Effect.Effect<A>);

/**
 * Marks an implementation of a Sync service written in JS: its operations
 * return their values. One written in Flick, as `S.Impl`, has runtime
 * functions, whose effects run at once; a value that is an `Eff` is data.
 */
export const syncAbi: unique symbol = /*#__PURE__*/ Symbol.for("@flick-lang/runtime/syncAbi");

/**
 * `S.op` of a Sync service: a function of the operation's arguments that
 * calls the implementation in the current context.
 */
export const syncOperation = <S extends object>(
  tag: Context.Service<S, S>,
  name: keyof S,
  arity: number,
  dicts: ReadonlyArray<unknown> = [],
): unknown => {
  const call = (args: ReadonlyArray<unknown>): unknown => {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker allows the call only where the service is
    const impl: S & { readonly [syncAbi]?: true } = Context.getUnsafe(current as Context.Context<S>, tag);
    let result: unknown = implementation(impl, name, dicts);
    for (const a of args) {
      if (typeof result !== "function") throw new TypeError("the operation takes fewer arguments");
      const next: unknown = Reflect.apply(result, undefined, [a]);
      result = next;
    }
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a Flick implementation's last stage is an Effect
    return impl[syncAbi] === true ? result : runIn(result as Effect.Effect<unknown>);
  };
  const collect = (args: ReadonlyArray<unknown>): unknown =>
    args.length === arity ? call(args) : (a: unknown) => collect([...args, a]);
  return arity === 0 ? call([]) : collect([]);
};

/** The services the Sync stage at hand reads. */
export const syncContext = (): Context.Context<never> => current;

/** The services of the fiber, for a sequence whose steps read them later. */
export const fiberContext: Effect.Effect<Context.Context<never>> = /*#__PURE__*/ Effect.context();

/**
 * The sequence of a `for` whose body makes Sync calls: each step runs with
 * `context`, the services of the stage that built it, so iterating it
 * again reads them again.
 */
export const seqIn = <A>(context: Context.Context<never>, gen: () => Iterator<A>): Seq<A> =>
  seq(function* () {
    const steps = gen();
    try {
      for (;;) {
        const step = syncIn(context, () => steps.next());
        if (step.done === true) return;
        yield step.value;
      }
    } finally {
      syncIn(context, () => steps.return?.());
    }
  });
