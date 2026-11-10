// The dispatch path knows only this optional observer. Span construction lives
// in the separately imported tracing module, so an untraced app can drop it.
import { Context, type Effect, type Tracer } from "effect";
import type { Handler } from "@flick-lang/runtime/ui/react/handlers";

export type ActionObserver = (
  handler: Handler,
  event: string,
  spans: Set<Tracer.Span>,
  task: () => Effect.Effect<unknown, unknown>,
) => Effect.Effect<unknown, unknown>;

export const ActionObserver = /*#__PURE__*/ Context.Reference<ActionObserver | undefined>("flick/ui/ActionObserver", {
  defaultValue: () => undefined,
});
