// The atom registry of Flick's own runtime. An atom describes a value; a
// registry computes it, keeps it, computes a derived one again when an atom it
// read changes, tells subscribers, and drops a value nothing holds. A change
// reaches dependents through a loop, not the stack, so a chain of thousands of
// them does not overflow it. A node is observed while it has a subscriber or
// an observed dependent: an observed node computes again at once when an atom
// it read changes, another only when it is read.
import type { Cause, Exit } from "../cause.ts";
import { type Ctx, forkNow } from "../fiber.ts";
import { withOwner } from "../owner.ts";

const ATOM = /*#__PURE__*/ Symbol.for("@flick-lang/runtime/rt/Atom");

export interface Atom<A = unknown> {
  readonly [ATOM]: true;
  read(reading: Reading): A;
  /** A state takes writes: what a write of `value` sets, through `setSelf`. */
  write?(value: A, setSelf: (value: A) => void): void;
  /** What a refresh of it refreshes instead of itself: a mapped atom's source. */
  refresh?(registry: Registry): void;
  /** Its value on a server, and in a root while it hydrates. */
  server?(get: (atom: Atom) => unknown): A;
  /** Whether its value stays while the registry lives, read or not. */
  keepAlive: boolean;
  readonly label?: string;
  /** How long its value stays after nothing holds it, in milliseconds. */
  readonly idleTtl?: number;
}

export const isAtom = (value: unknown): value is Atom => typeof value === "object" && value !== null && ATOM in value;

/** A readable atom of `read`. */
export const readable = <A>(read: (reading: Reading) => A, refresh?: (registry: Registry) => void): Atom<A> =>
  refresh === undefined ? { [ATOM]: true, read, keepAlive: false } : { [ATOM]: true, read, refresh, keepAlive: false };

/** A state: `initial` until written. */
export const state = <A>(initial: A): Atom<A> => ({
  ...readable(() => initial),
  write: (value, setSelf) => setSelf(value),
});

/** `atom` changed by `patch`: a new atom, as a label or `keepAlive` makes one. */
export const patched = <A>(atom: Atom<A>, patch: Partial<Omit<Atom<A>, typeof ATOM>>): Atom<A> => ({ ...atom, ...patch });

/** The values of `source` through `f`; a refresh refreshes the source. */
export const mapped = <A, B>(source: Atom<A>, f: (a: A) => B): Atom<B> =>
  readable(
    (reading) => f(reading.get(source)),
    (registry) => registry.refresh(source),
  );

/** What a read function gets: the atoms it reads through it are its dependencies. */
export class Reading {
  readonly registry: Registry;
  readonly node: Node;
  disposed = false;
  /** A value set while the read function runs, which replaces what it returns. */
  early: { readonly value: unknown } | undefined = undefined;
  /** The parents it read: a read function drops the others. */
  readonly reads = new Set<Node>();
  private finalizers: Array<() => void> = [];
  constructor(registry: Registry, node: Node) {
    this.registry = registry;
    this.node = node;
  }
  /** The value of `atom`, which this read then depends on. */
  get<B>(atom: Atom<B>): B {
    const parent = this.registry.node(atom);
    if (!this.disposed && this.node.reading === this) {
      link(this.node, parent);
      this.reads.add(parent);
    }
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the node of `atom` holds a value of its type
    return this.registry.valueOf(parent) as B;
  }
  /** The node's value before this read, if it had one. */
  self(): { readonly value: unknown } | undefined {
    return this.node.state === UNINITIALIZED ? undefined : { value: this.node.value };
  }
  setSelf(value: unknown): void {
    if (this.disposed || this.node.reading !== this) return;
    if (this.node.computing) this.early = { value };
    else this.registry.setValue(this.node, value);
  }
  addFinalizer(finalizer: () => void): void {
    if (this.disposed) finalizer();
    else this.finalizers.push(finalizer);
  }
  dispose(): void {
    this.disposed = true;
    let failed: { readonly error: unknown } | undefined;
    for (const f of this.finalizers.splice(0).toReversed()) {
      try { f(); }
      catch (error) { failed ??= { error }; }
    }
    if (failed !== undefined) throw failed.error;
  }
}

const UNINITIALIZED = 0;
const STALE = 1;
const VALID = 2;
const REMOVED = 3;

export class Node {
  readonly atom: Atom;
  state: 0 | 1 | 2 | 3 = UNINITIALIZED;
  value: unknown = undefined;
  computing = false;
  reading: Reading | undefined = undefined;
  readonly parents = new Set<Node>();
  readonly children = new Set<Node>();
  readonly listeners = new Set<() => void>();
  observedChildren = 0;
  constructor(atom: Atom) {
    this.atom = atom;
  }
  get observed(): boolean {
    return this.listeners.size > 0 || this.observedChildren > 0;
  }
  get removable(): boolean {
    return !this.atom.keepAlive && this.listeners.size === 0 && this.children.size === 0;
  }
}

/** Moves the count of observed children of each parent of `from` by `delta`, up as far as a parent turns. */
const observedBy = (from: Node, delta: 1 | -1): void => {
  const turned = [from];
  for (let node = turned.pop(); node !== undefined; node = turned.pop()) {
    for (const parent of node.parents) {
      const was = parent.observed;
      parent.observedChildren += delta;
      if (was !== parent.observed) turned.push(parent);
    }
  }
};

const link = (child: Node, parent: Node): void => {
  if (child.parents.has(parent)) return;
  child.parents.add(parent);
  parent.children.add(child);
  if (child.observed) {
    const was = parent.observed;
    parent.observedChildren++;
    if (!was) observedBy(parent, 1);
  }
};

const unlink = (child: Node, parent: Node): void => {
  if (!child.parents.delete(parent)) return;
  parent.children.delete(child);
  if (child.observed) {
    parent.observedChildren--;
    if (!parent.observed) observedBy(parent, -1);
  }
};

/** Listeners wait while a change is under way, so each hears the end of it once. */
let depth = 0;
const announced = new Set<Node>();

/** Runs `f` and then tells the subscribers of what it changed, each once. */
export const batch = <A>(f: () => A): A => {
  depth++;
  try {
    return f();
  } finally {
    if (--depth === 0) flush();
  }
};

const flush = (): void => {
  depth++;
  let failed: { readonly error: unknown } | undefined;
  try {
    for (let node = first(announced); node !== undefined; node = first(announced)) {
      announced.delete(node);
      for (const listener of Array.from(node.listeners)) {
        try { listener(); }
        catch (error) { failed ??= { error }; }
      }
    }
  } finally {
    depth--;
  }
  if (failed !== undefined) throw failed.error;
};

const first = <A>(set: ReadonlySet<A>): A | undefined => set.values().next().value;

export class Registry {
  /** The evidence the tasks of its atoms run with: where it was built. */
  readonly ctx: Ctx;
  private readonly nodes = new Map<Atom, Node>();
  /** What a mutation of a key refreshes: the queries read with it. */
  private readonly queries = new Map<string, Set<() => void>>();
  private disposed = false;
  constructor(ctx: Ctx) {
    this.ctx = ctx;
  }

  node(atom: Atom): Node {
    let node = this.nodes.get(atom);
    if (node === undefined) {
      if (this.disposed) throw new Error(`the registry of the atom ${atom.label ?? ""} is disposed`);
      node = new Node(atom);
      this.nodes.set(atom, node);
      this.dropLater(node);
    }
    return node;
  }

  valueOf(node: Node): unknown {
    if (node.state === STALE) this.computeStaleParents(node);
    if (node.state !== VALID) this.compute(node);
    return node.value;
  }

  /** Computes the stale atoms `node` read, from the top down, so its own read gets each at once, not through the stack. */
  private computeStaleParents(node: Node): void {
    const above: Array<Node> = [];
    const seen = new Set<Node>([node]);
    const stack: Array<readonly [Node, Iterator<Node>]> = [[node, node.parents.values()]];
    for (let top = stack.at(-1); top !== undefined; top = stack.at(-1)) {
      const next = top[1].next();
      if (next.done === true) {
        stack.pop();
        if (top[0] !== node) above.push(top[0]);
      } else if (next.value.state === STALE && !seen.has(next.value)) {
        seen.add(next.value);
        stack.push([next.value, next.value.parents.values()]);
      }
    }
    for (const parent of above) if (parent.state === STALE) this.compute(parent);
  }

  get<A>(atom: Atom<A>): A {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the node of `atom` holds a value of its type
    return this.valueOf(this.node(atom)) as A;
  }

  set<A>(atom: Atom<A>, value: A): void {
    const node = this.node(atom);
    if (atom.write === undefined) throw new TypeError(`the atom ${atom.label ?? ""} takes no writes`);
    batch(() => atom.write?.(value, (v) => this.setValue(node, v)));
  }

  update<A>(atom: Atom<A>, f: (a: A) => A): void {
    this.set(atom, f(this.get(atom)));
  }

  modify<A, B>(atom: Atom<A>, f: (a: A) => readonly [B, A]): B {
    const [result, next] = f(this.get(atom));
    this.set(atom, next);
    return result;
  }

  /** Computes `atom` again: a task atom runs its task anew. */
  refresh(atom: Atom): void {
    if (atom.refresh !== undefined) return atom.refresh(this);
    const node = this.node(atom);
    if (node.state !== VALID) return;
    this.stale(node);
    batch(() => this.propagate([node]));
  }

  /** Calls `f` with the value of `atom` whenever it changes, at once too with `immediate`. */
  subscribe<A>(atom: Atom<A>, f: (value: A) => void, immediate = false): () => void {
    const node = this.node(atom);
    const value = this.valueOf(node);
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the node of `atom` holds a value of its type
    const listener = (): void => f(this.valueOf(node) as A);
    const was = node.observed;
    node.listeners.add(listener);
    if (!was) observedBy(node, 1);
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- as above
    if (immediate) f(value as A);
    return () => {
      if (node.listeners.delete(listener) && !node.observed) observedBy(node, -1);
      this.dropLater(node);
    };
  }

  /** Holds `atom`'s value until the function returned is called. */
  mount(atom: Atom): () => void {
    return this.subscribe(atom, () => undefined, true);
  }

  /** The value of `atom` as a server renders it. */
  serverValue(atom: Atom): unknown {
    return atom.server === undefined ? this.get(atom) : atom.server((a) => this.get(a));
  }

  /** `f` runs whenever a mutation names one of `keys`, until the function returned is called. */
  register(keys: ReadonlyArray<string>, f: () => void): () => void {
    for (const key of keys) {
      const set = this.queries.get(key) ?? new Set();
      set.add(f);
      this.queries.set(key, set);
    }
    return () => {
      for (const key of keys) this.queries.get(key)?.delete(f);
    };
  }

  /** Runs anew, once each, the queries of `keys`. */
  invalidate(keys: ReadonlyArray<string>): void {
    const runs = new Set(keys.flatMap((key) => [...(this.queries.get(key) ?? [])]));
    let failed: { readonly error: unknown } | undefined;
    try {
      batch(() => {
        for (const run of runs) {
          try { run(); }
          catch (error) { failed ??= { error }; }
        }
      });
    } catch (error) { failed ??= { error }; }
    if (failed !== undefined) throw failed.error;
  }

  dispose(): void {
    this.disposed = true;
    let failed: { readonly error: unknown } | undefined;
    for (const node of this.nodes.values()) {
      node.state = REMOVED;
      try { node.reading?.dispose(); }
      catch (error) { failed ??= { error }; }
    }
    this.nodes.clear();
    this.queries.clear();
    if (failed !== undefined) throw failed.error;
  }

  setValue(node: Node, value: unknown): void {
    if (node.state === REMOVED || (node.state === VALID && Object.is(node.value, value))) return;
    // a first value replaces none a dependent holds
    const initialized = node.state !== UNINITIALIZED;
    node.value = value;
    node.state = VALID;
    batch(() => {
      if (node.listeners.size > 0) announced.add(node);
      if (initialized) this.propagate(this.staleChildren(node));
    });
  }

  /** Computes `node`, whose dependents, if any, are stale already; its subscribers hear of a new value. */
  private compute(node: Node): void {
    node.reading?.dispose();
    const reading = new Reading(this, node);
    node.reading = reading;
    node.computing = true;
    let value: unknown;
    try {
      value = node.atom.read(reading);
    } finally {
      node.computing = false;
      for (const parent of Array.from(node.parents)) {
        if (reading.reads.has(parent)) continue;
        unlink(node, parent);
        this.dropLater(parent);
      }
    }
    if (node.reading !== reading) return;
    if (reading.early !== undefined) value = reading.early.value;
    const fresh = node.state === UNINITIALIZED || !Object.is(node.value, value);
    node.value = value;
    node.state = VALID;
    if (fresh && node.listeners.size > 0) batch(() => announced.add(node));
  }

  /** Marks `node` stale, ending what its last read started. */
  private stale(node: Node): void {
    if (node.state !== VALID) return;
    node.state = STALE;
    const reading = node.reading;
    node.reading = undefined;
    reading?.dispose();
  }

  /** The dependents of `node` that held its value, now stale. */
  private staleChildren(node: Node): Array<Node> {
    const stale: Array<Node> = [];
    for (const child of Array.from(node.children)) {
      if (child.state !== VALID) continue;
      this.stale(child);
      stale.push(child);
    }
    return stale;
  }

  /**
   * The stale nodes of `queue`, breadth first: an observed one computes again
   * at once, and its dependents follow if its value changed; another stays
   * stale until read, its dependents with it.
   */
  private propagate(queue: Array<Node>): void {
    let failure: { readonly error: unknown } | undefined;
    for (let i = 0; i < queue.length; i++) {
      const node = queue[i];
      if (node === undefined || node.state !== STALE) continue;
      if (node.observed) {
        const before = node.value;
        try {
          this.compute(node);
        } catch (error) {
          failure ??= { error };
          continue;
        }
        if (Object.is(before, node.value)) continue;
      }
      for (const child of this.staleChildren(node)) queue.push(child);
    }
    if (failure !== undefined) throw failure.error;
  }

  /** Drops `node` once nothing holds it, on a later turn, or after its idle time. */
  private dropLater(node: Node): void {
    if (node.atom.keepAlive) return;
    const drop = (): void => {
      if (this.nodes.get(node.atom) === node && node.removable) this.remove(node);
    };
    if (node.atom.idleTtl === undefined) queueMicrotask(drop);
    else setTimeout(drop, node.atom.idleTtl);
  }

  private remove(node: Node): void {
    this.nodes.delete(node.atom);
    node.state = REMOVED;
    let failed: { readonly error: unknown } | undefined;
    try { node.reading?.dispose(); }
    catch (error) { failed = { error }; }
    node.reading = undefined;
    for (const parent of Array.from(node.parents)) {
      unlink(node, parent);
      this.dropLater(parent);
    }
    if (failed !== undefined) throw failed.error;
  }
}

/** `Reactivity.AsyncResult`: a task's result as an atom holds it. */
export type AsyncResult =
  | { readonly _tag: "Initial"; readonly waiting: boolean }
  | { readonly _tag: "Success"; readonly value: unknown; readonly waiting: boolean }
  | {
      readonly _tag: "Failure";
      readonly cause: Cause;
      readonly previous: { readonly value: unknown } | undefined;
      readonly waiting: boolean;
    };

const isResult = (value: unknown): value is AsyncResult =>
  typeof value === "object" && value !== null && ["Initial", "Success", "Failure"].includes(String(Reflect.get(value, "_tag")));

/** The result of a run that has started again after `previous`. */
const waitingFrom = (previous: { readonly value: unknown } | undefined): AsyncResult =>
  previous !== undefined && isResult(previous.value) ? { ...previous.value, waiting: true } : { _tag: "Initial", waiting: true };

const lastSuccess = (previous: { readonly value: unknown } | undefined): { readonly value: unknown } | undefined => {
  const result = previous?.value;
  if (!isResult(result) || result._tag === "Initial") return undefined;
  return result._tag === "Success" ? { value: result.value } : result.previous;
};

const resultOf = (exit: Exit, previous: { readonly value: unknown } | undefined): AsyncResult =>
  exit.cause === undefined
    ? { _tag: "Success", value: exit.value, waiting: false }
    : { _tag: "Failure", cause: exit.cause, previous: lastSuccess(previous), waiting: false };

/**
 * The atom of the task `make` gives: run in a scope of its own, with the
 * evidence of the registry, by the registry that reads it; interrupted when
 * the atom computes again or is dropped.
 */
export const taskAtom = (make: (reading: Reading) => () => unknown): Atom<AsyncResult> =>
  readable((reading) => {
    const task = make(reading);
    const previous = reading.self();
    const fiber = forkNow(() => withOwner(() => task()), reading.registry.ctx);
    if (fiber.exit !== undefined) return resultOf(fiber.exit, previous);
    fiber.observe((exit) => reading.setSelf(resultOf(exit, previous)));
    reading.addFinalizer(() => fiber.interrupt(-1));
    return waitingFrom(previous);
  });
