// Per-root opt-in; capturing the reference at mount keeps event dispatch cheap.
import { Effect } from "effect";
import { ActionObserver } from "./action-observer.ts";

const traceAction: ActionObserver = (handler, event, spans, task) =>
  Effect.useSpan(handler.label ?? "Ui.event", {
    attributes: {
      "flick.action.policy": handler.policy.kind,
      ...(handler.policy.kind === "queue" ? { "flick.action.capacity": handler.policy.capacity } : {}),
      ...(handler.policy.kind === "concurrent" ? { "flick.action.limit": handler.policy.limit } : {}),
      "flick.event": event,
    },
  }, (span) => {
    spans.add(span);
    return Effect.withFiber((fiber) => {
      span.attribute("flick.fiber", fiber.id);
      return task();
    }).pipe(
      Effect.withParentSpan(span),
      Effect.onInterrupt(() => Effect.sync(() => span.event("interrupted", BigInt(Date.now()) * 1_000_000n))),
      Effect.ensuring(Effect.sync(() => spans.delete(span))),
    );
  });

export const withActionSpans = (enabled: boolean) => <A, E, R>(task: Effect.Effect<A, E, R>): Effect.Effect<A, E, R> =>
  Effect.provideService(task, ActionObserver, enabled ? traceAction : undefined);
