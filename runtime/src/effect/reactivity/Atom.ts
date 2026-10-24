// Effect's `effect/reactivity` atoms for the bindings of the standard
// library (unstable in Effect 4.0.5). A state atom is built from its value
// alone, so a function or an effect as the value stays a value; the
// registry is the Flick service `Reactivity.Atom.Registry`, whose operations
// read and write through Effect's registry.
import { type Context, type Duration, Effect, Layer } from "effect";
import * as Atom from "effect/reactivity/Atom";
import * as AtomRegistry from "effect/reactivity/AtomRegistry";

export * from "effect/reactivity/Atom";

/** `Reactivity.Atom.state`: a writable atom holding `value`. */
export const flickState = <A>(value: A): Atom.Writable<A> =>
  Atom.writable(
    () => value,
    (ctx, next: A) => ctx.setSelf(next),
  );

/** `Reactivity.Atom.read`: the value of `atom` inside a read function, which then depends on it. */
export const flickRead = <A>(ctx: Atom.AtomContext, atom: Atom.Atom<A>): A => ctx(atom);

/** `Reactivity.Atom.from_task`: the atom of an effect's result, run in the registry. */
export const flickFromTask = <A, E>(task: Effect.Effect<A, E>) => Atom.make(task);

/** `Reactivity.Atom.map`. */
export const flickMap = <A, B>(f: (a: A) => B, atom: Atom.Atom<A>): Atom.Atom<B> => Atom.map(atom, f);

/** `Reactivity.Atom.with_label`: names the atom in debugging tools. */
export const flickWithLabel = <T extends Atom.Atom<unknown>>(name: string, atom: T): T => Atom.withLabel(atom, name);

/** `Reactivity.Atom.idle_ttl`: how long the value stays after nobody reads it. */
export const flickIdleTtl = <T extends Atom.Atom<unknown>>(ttl: Duration.Duration, atom: T): T =>
  Atom.setIdleTTL(atom, ttl);

/** `Reactivity.Atom.readonly`: a writable atom as an atom to read. */
export const flickReadonly = <A>(atom: Atom.Writable<A>): Atom.Atom<A> => atom;

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

/** `Reactivity.Atom.layer`: a fresh registry as the service `key`. */
export const flickLayer = <I>(key: Context.Key<I, ReturnType<typeof impl>>): Layer.Layer<I> =>
  Layer.effect(key, Effect.map(Effect.service(AtomRegistry.AtomRegistry), impl)).pipe(
    Layer.provide(AtomRegistry.layer),
  );
