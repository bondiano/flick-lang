// Sync stages: a function whose services are `Atom.Read` alone, with no
// errors, runs at once instead of building an Effect. Its operations read
// the services of the context set around it: by `syncIn` where the runtime
// calls it (a render, a derived atom), by `sync` inside a fiber.
import { Context, Effect } from "effect";

import { type Seq, seq } from "./core/seq.ts";

let current: Context.Context<never> = Context.empty();

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

/** A runtime call in a Sync stage: runs at once with the context's services. */
export const runIn = <A>(effect: Effect.Effect<A, unknown, unknown>): A =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker allows only the stage's services
  Effect.runSyncWith(current)(effect as Effect.Effect<A>);

/**
 * `S.op` of a Sync service: a function of the operation's arguments that
 * calls the implementation in the current context; an implementation
 * written as runtime functions has its effect run at once.
 */
export const syncOperation = <S extends object>(tag: Context.Service<S, S>, name: keyof S, arity: number): unknown => {
  const call = (args: ReadonlyArray<unknown>): unknown => {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker allows the call only where the service is
    const impl = Context.getUnsafe(current as Context.Context<S>, tag);
    let result: unknown = impl[name];
    for (const a of args) {
      if (typeof result !== "function") throw new TypeError("the operation takes fewer arguments");
      const next: unknown = Reflect.apply(result, undefined, [a]);
      result = next;
    }
    return Effect.isEffect(result) ? runIn(result) : result;
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
