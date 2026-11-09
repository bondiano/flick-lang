// Effect's registry ABI, with stack-safe observation and transactional writes.
import { batch } from "effect/reactivity/Atom";
import type * as Registry from "effect/reactivity/AtomRegistry";

export * from "effect/reactivity/AtomRegistry";

/**
 * Effect 4.1.2 propagates observation recursively. Walk its parent edges on
 * the heap, preserving one observed-child count per edge, and batch commits
 * so dependent values settle before listeners run. These fields and methods
 * are version-pinned NodeImpl internals; shared prototypes stay unchanged.
 */
interface ManagedNode extends Registry.Node<unknown> {
  parents: Set<ManagedNode>;
  readonly isObserved: boolean;
  observedChildren: number;
  observedChanged(delta: number): void;
  invalidateChildren(): void;
}

/** The layer installs this callback before the registry's first read. */
export const prepareNode = (node: Registry.Node<unknown>): void => {
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- Effect 4.1.2 NodeImpl
  const managed = node as ManagedNode;
  managed.observedChanged = (delta: number): void => {
    const pending = [...managed.parents];
    for (const parent of pending) {
      const before = parent.isObserved;
      parent.observedChildren += delta;
      if (before !== parent.isObserved) pending.push(...parent.parents);
    }
  };
  managed.invalidateChildren = batch.bind(managed, managed.invalidateChildren.bind(managed));
};
