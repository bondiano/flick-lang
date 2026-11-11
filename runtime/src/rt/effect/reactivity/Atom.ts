// `Reactivity.Atom` on Flick's own runtime: atoms of the kernel's registry. A
// state is built from its value alone, so a function or a task as the value
// stays a value.
import type { Codec } from "../../schema.ts";
import { serializable } from "../../atoms/serial.ts";
import type { Duration } from "../../duration.ts";
import { type Atom, type Reading, mapped, patched, readable as make, state, taskAtom } from "../../atoms/registry.ts";

export { atomLayer, flickLayer } from "../../atoms/services.ts";

/** `Reactivity.Atom.state`. */
export const flickState = <A>(value: A): Atom<A> => state(value);

/** `Reactivity.Atom.readable`: `read` computes its value from the atoms it reads through the context. */
export const readable = <A>(read: (reading: Reading) => A): Atom<A> => make(read);

/** `Reactivity.Atom.read`: the value of `atom` inside a read function, which then depends on it. */
export const flickRead = <A>(reading: Reading, atom: Atom<A>): A => reading.get(atom);

/** `Reactivity.Atom.readonly`. */
export const flickReadonly = <A>(atom: Atom<A>): Atom<A> => atom;

/** `Reactivity.Atom.from_task`: the atom of a task's result, run by the registry that reads it. */
export const flickFromTask = (task: () => unknown) => taskAtom(() => task);

/** `Reactivity.Atom.map`. */
export const flickMap = <A, B>(f: (a: A) => B, atom: Atom<A>): Atom<B> => mapped(atom, f);

/** `Reactivity.Atom.family`: one atom per key, made by `f` the first time; strings and numbers compare by value. */
export const family = <K, A>(f: (key: K) => A): ((key: K) => A) => {
  const atoms = new Map<K, { deref(): A | undefined }>();
  return (key) => {
    const known = atoms.get(key)?.deref();
    if (known !== undefined) return known;
    const atom = f(key);
    atoms.set(key, typeof atom === "object" && atom !== null ? new WeakRef(atom) : { deref: () => atom });
    return atom;
  };
};

/** `Reactivity.Atom.with_label`. */
export const flickWithLabel = <A>(label: string, atom: Atom<A>): Atom<A> => patched(atom, { label });

/** `Reactivity.Atom.keep_alive`. */
export const keepAlive = <A>(atom: Atom<A>): Atom<A> => patched(atom, { keepAlive: true });

/** `Reactivity.Atom.idle_ttl`. */
export const flickIdleTtl = <A>(ttl: Duration, atom: Atom<A>): Atom<A> => patched(atom, { idleTtl: ttl.millis });

/** `Reactivity.Atom.serializable`: the state under `key`, coded by a JSON codec for hydration. */
export const flickSerializable = <A>(key: string, codec: Codec<A>, atom: Atom<A>): Atom<A> => serializable(key, codec, atom);

/** `Reactivity.Atom.serializable_family`: one serializable state per key, under `prefix/key`. */
export const flickSerializableFamily = <A>(prefix: string, codec: Codec<A>, f: (key: string) => Atom<A>): ((key: string) => Atom<A>) =>
  family((key: string) => serializable(`${prefix}/${key}`, codec, f(key)));
