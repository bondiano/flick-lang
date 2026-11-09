// The bindings of `Scope` on the kernel: finalizers added to the owner of a call.
import { none, some } from "../core/data.ts";
import type { Exit } from "../cause.ts";
import { RawCause } from "../outcome.ts";
import { addFinalizer, addFinalizerExit } from "../owner.ts";

export const flickAddFinalizer = (finalizer: () => unknown): unknown => addFinalizer(finalizer);

export const flickAddFinalizerExit = (finalizer: (cause: unknown) => unknown): unknown =>
  addFinalizerExit((exit: Exit) => finalizer(exit.cause === undefined ? none : some(new RawCause(exit.cause))));
