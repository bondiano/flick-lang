// Effect's `effect/reactivity` atoms for the bindings of the standard
// library (unstable in Effect 4.1.2). A state atom is built from its value
// alone, so a function or an effect as the value stays a value; the
// registry is the Flick service `Reactivity.Atom.Registry`, whose operations
// read and write through Effect's registry.
import { Context, type Duration, Effect, Layer, LogLevel, Option, type Schema, Scope, type Tracer } from "effect";
import type * as AsyncResult from "effect/reactivity/AsyncResult";
import * as Atom from "effect/reactivity/Atom";
import * as AtomRegistry from "effect/reactivity/AtomRegistry";

import { service } from "../../effects.ts";

export * from "effect/reactivity/Atom";

/**
 * What the tasks of each registry run with: the services and references
 * where its layer was built, so a task run while a server renders a page
 * is part of the request's trace and carries its headers.
 */
const callers = new WeakMap<AtomRegistry.AtomRegistry, Context.Context<never>>();

/** The atom of the task `make` gives, run with what its registry's layer was built with; its scope is the atom's. */
export const taskAtomOf = <A, E>(make: (get: Atom.AtomContext) => Effect.Effect<A, E>): Atom.Atom<AsyncResult.AsyncResult<A, E>> =>
  Atom.make((get) => {
    const task = make(get);
    const caller = callers.get(get.registry);
    return caller === undefined ? task : Effect.provideContext(task, caller);
  });

/** The atom of `task`. */
export const taskAtom = <A, E>(task: Effect.Effect<A, E>): Atom.Atom<AsyncResult.AsyncResult<A, E>> => taskAtomOf(() => task);

/** Records what the tasks of `registry` run with. */
const recordCaller = (registry: AtomRegistry.AtomRegistry) =>
  Effect.map(Effect.context(), (context) => {
    callers.set(registry, Context.omit(Scope.Scope)(context));
  });

/** `Reactivity.Atom.state`: a writable atom holding `value`. */
export const flickState = <A>(value: A): Atom.Writable<A> =>
  Atom.writable(
    () => value,
    (ctx, next: A) => ctx.setSelf(next),
  );

/** `Reactivity.Atom.read`: the value of `atom` inside a read function, which then depends on it. */
export const flickRead = <A>(ctx: Atom.AtomContext, atom: Atom.Atom<A>): A => ctx(atom);

/** `Reactivity.Atom.from_task`: the atom of an effect's result, run in the registry. */
export const flickFromTask = <A, E>(task: Effect.Effect<A, E>) => taskAtom(task);

/** `Reactivity.Atom.map`. */
export const flickMap = <A, B>(f: (a: A) => B, atom: Atom.Atom<A>): Atom.Atom<B> => Atom.map(atom, f);

/** `Reactivity.Atom.with_label`: names the atom in debugging tools. */
export const flickWithLabel = <T extends Atom.Atom<unknown>>(name: string, atom: T): T => Atom.withLabel(atom, name);

/** `Reactivity.Atom.serializable`: the state under `key`, coded by a JSON codec for hydration. */
export const flickSerializable = <A>(key: string, codec: Schema.Codec<A, unknown>, atom: Atom.Writable<A>): Atom.Writable<A> =>
  Atom.serializable(atom, { key, schema: codec });

/** `Reactivity.Atom.serializable_family`: one serializable state per key, under `prefix/key`. */
export const flickSerializableFamily = <A>(
  prefix: string,
  codec: Schema.Codec<A, unknown>,
  make: (key: string) => Atom.Writable<A>,
): ((key: string) => Atom.Writable<A>) =>
  Atom.family((key: string) => flickSerializable(`${prefix}/${key}`, codec, make(key)));

/** `Reactivity.Atom.idle_ttl`: how long the value stays after nobody reads it. */
export const flickIdleTtl = <T extends Atom.Atom<unknown>>(ttl: Duration.Duration, atom: T): T =>
  Atom.setIdleTTL(atom, ttl);

/** `Reactivity.Atom.readonly`: a writable atom as an atom to read. */
export const flickReadonly = <A>(atom: Atom.Writable<A>): Atom.Atom<A> => atom;

/** `atom ← action ← event ← span (fiber) at place`, from the span of the task that writes. */
const describeWrite = (atom: Atom.Atom<unknown>, span: Option.Option<Tracer.Span>, place: string): string => {
  const name = atom.label?.[0] ?? "an atom";
  const cause = Option.match(span, {
    onNone: () => "task",
    onSome: (s) => {
      const event = s.attributes.get("flick.event");
      const fiber = s.attributes.get("flick.fiber");
      return [
        `action ${s.name}`,
        ...(typeof event === "string" ? [`event ${event}`] : []),
        `span ${s.spanId}${typeof fiber === "number" ? ` (fiber #${fiber})` : ""}`,
      ].join(" ← ");
    },
  });
  return `atom ${name} ← ${cause}${place === "" ? "" : ` at ${place}`}`;
};

/** `write` of `atom` at `place`, which a fiber that logs debug logs with its cause. */
const traced = <A>(atom: Atom.Atom<unknown>, place: string, write: Effect.Effect<A>): Effect.Effect<A> =>
  Effect.flatMap(LogLevel.isEnabled("Debug"), (debug) =>
    debug
      ? Effect.option(Effect.currentSpan).pipe(
          Effect.flatMap((span) => Effect.logDebug(describeWrite(atom, span, place))),
          Effect.andThen(write),
        )
      : write,
  );

/** The operations of `Reactivity.Atom.Registry` over Effect's registry. */
const impl = (registry: AtomRegistry.AtomRegistry) => ({
  get: <A>(atom: Atom.Atom<A>) => Effect.sync(() => registry.get(atom)),
  set: <A>(atom: Atom.Writable<A>) => (value: A) => Effect.sync(() => registry.set(atom, value)),
  update: <A>(atom: Atom.Writable<A>) => (f: (a: A) => A) => Effect.sync(() => registry.update(atom, f)),
  modify:
    <A, B>(atom: Atom.Writable<A>) =>
    (f: (a: A) => readonly [B, A]) =>
      Effect.sync(() =>
        registry.modify(atom, (a: A): [B, A] => {
          const [result, next] = f(a);
          return [result, next];
        }),
      ),
  refresh: <A>(atom: Atom.Atom<A>) => Effect.sync(() => registry.refresh(atom)),
});

/** The service `Reactivity.Atom.Registry`. */
export const RegistryService = /*#__PURE__*/ service<ReturnType<typeof impl>>("Reactivity.Atom.Registry");

/** Effect's registry behind each implementation of the service, for views that subscribe. */
const natives = new WeakMap<object, AtomRegistry.AtomRegistry>();

/** The registry of an implementation of `Reactivity.Atom.Registry`. */
export const nativeRegistry = (record: object): AtomRegistry.AtomRegistry | undefined => natives.get(record);

/**
 * The service `Atom.Write`: the writes of `Reactivity.Atom.Registry`, each
 * at its place in the source, and its reads without tracking.
 */
const writeImpl = (record: ReturnType<typeof impl>) => ({
  set:
    (place: string) =>
    <A>(atom: Atom.Writable<A>) =>
    (value: A) =>
      traced(atom, place, record.set(atom)(value)),
  update:
    (place: string) =>
    <A>(atom: Atom.Writable<A>) =>
    (f: (a: A) => A) =>
      traced(atom, place, record.update(atom)(f)),
  modify:
    (place: string) =>
    <A, B>(atom: Atom.Writable<A>) =>
    (f: (a: A) => readonly [B, A]): Effect.Effect<B> =>
      traced(atom, place, record.modify<A, B>(atom)(f)),
  current: record.get,
  refresh: record.refresh,
});

/** The service `Atom.Write`. */
export const WriteService = /*#__PURE__*/ service<ReturnType<typeof writeImpl>>("Atom.Write");

/** `Atom.layer`: a fresh registry as `Reactivity.Atom.Registry` and as `Atom.Write`. */
export const atomLayer = /*#__PURE__*/ Layer.effectContext(
  Effect.map(Effect.tap(Effect.service(AtomRegistry.AtomRegistry), recordCaller), (registry) => {
    const record = impl(registry);
    const write = writeImpl(record);
    natives.set(record, registry);
    natives.set(write, registry);
    return Context.make(RegistryService, record).pipe(Context.add(WriteService, write));
  }),
).pipe(Layer.provide(AtomRegistry.layer));

/** `Reactivity.Atom.layer`: a fresh registry as the service `key`. */
export const flickLayer = <I>(key: Context.Key<I, ReturnType<typeof impl>>): Layer.Layer<I> =>
  Layer.effect(
    key,
    Effect.map(Effect.tap(Effect.service(AtomRegistry.AtomRegistry), recordCaller), (registry) => {
      const record = impl(registry);
      natives.set(record, registry);
      return record;
    }),
  ).pipe(Layer.provide(AtomRegistry.layer));

/** The labelled states, by label: a module replaced in development finds the new one of each. */
const states = /*#__PURE__*/ new Map<string, WeakRef<Atom.Writable<unknown>>>();

/** The labelled states `registry` holds and their values. */
export const statesOf = (registry: AtomRegistry.AtomRegistry): Map<string, unknown> => {
  const values = new Map<string, unknown>();
  registry.getNodes().forEach((node) => {
    const name: unknown = Array.isArray(node.atom.label) ? node.atom.label[0] : undefined;
    if (typeof name === "string" && states.get(name)?.deref() === node.atom) values.set(name, node.value());
  });
  return values;
};

/** Gives the labelled states of `values` their values in `registry`. */
export const seedStates = (registry: AtomRegistry.AtomRegistry, values: ReadonlyMap<string, unknown>): void => {
  for (const [name, value] of values) {
    const atom = states.get(name)?.deref();
    if (atom !== undefined) registry.set(atom, value);
  }
};

/** Records the labelled state `atom` under `name`. */
export const keepState = (name: string, atom: Atom.Writable<unknown>): void => {
  states.set(name, new WeakRef(atom));
};
