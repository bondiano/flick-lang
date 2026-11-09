// The bindings of `Fiber` on the kernel, over a child spawned into an owner.
import { type Exit } from "../cause.ts";
import { type Fiber, type Run, asExit, awaitFiber, currentFiber, raiseOf } from "../fiber.ts";
import type { Option } from "../../core/data.ts";
import type { FlickFiber } from "../owner.ts";

export function* flickAwait(fiber: FlickFiber): Run {
  return yield awaitFiber(fiber.fiber);
}

export function* flickAwaitAll(fibers: ReadonlyArray<FlickFiber>): Run {
  const exits: Array<Exit> = [];
  for (const f of fibers) exits.push(asExit(yield awaitFiber(f.fiber)));
  return exits;
}

export function* flickJoinAll(fibers: ReadonlyArray<FlickFiber>): Run {
  const out: Array<unknown> = [];
  for (const f of fibers) out.push(raiseOf(asExit(yield awaitFiber(f.fiber))));
  return out;
}

/** Asks each to stop, as `by`, and waits for them all. */
function* stopping(fibers: ReadonlyArray<Fiber>, by: number): Run {
  for (const f of fibers) f.interrupt(by);
  for (const f of fibers) yield awaitFiber(f);
  return undefined;
}

export const flickInterrupt = (fiber: FlickFiber): Run => stopping([fiber.fiber], currentFiber().id);

export const flickInterruptAs =
  (fiberId: Option<number>) =>
  (fiber: FlickFiber): Run =>
    stopping([fiber.fiber], fiberId._tag === "Some" ? fiberId.value : currentFiber().id);

export const flickInterruptAll = (fibers: ReadonlyArray<FlickFiber>): Run =>
  stopping(
    fibers.map((f) => f.fiber),
    currentFiber().id,
  );

export const flickInterruptAllAs =
  (fiberId: number) =>
  (fibers: ReadonlyArray<FlickFiber>): Run =>
    stopping(
      fibers.map((f) => f.fiber),
      fiberId,
    );
