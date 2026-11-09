// React-only rendering and event ownership. Adapters mount roots; this core
// stops events first, then interrupts and joins tasks before its scope closes.
import { Cause, Context, Effect, Fiber } from "effect";
import type * as AtomRegistry from "../../effect/reactivity/AtomRegistry.ts";
import { Component, type ReactNode, createElement } from "react";

import type { Failure } from "../../cause.ts";
import { hotHooks } from "../../hot.ts";
import { RegistryService, nativeRegistry, seedStates, statesOf } from "../../effect/reactivity/Atom.ts";
import { DocumentService, document } from "./document.ts";
import type { Ui } from "@flick-lang/runtime/ui/react/nodes";
import { Bridge } from "./policies.ts";
import { BridgeContext, RegistryContext, toReact } from "./render.ts";
import type { RenderRoot, Renderer } from "@flick-lang/runtime/ui/react/tree";

export * from "@flick-lang/runtime/ui/react/nodes";
export type { Renderer } from "@flick-lang/runtime/ui/react/tree";
export * from "./links.ts";
export { DocumentService } from "./document.ts";
export { type Events, uiRenderStatic } from "./render.ts";
export * from "./views.ts";
export {
  actionConcurrent,
  actionExhaust,
  actionLatest,
  actionQueue,
  actionWith,
  eventConcurrent,
  eventExhaust,
  eventLatest,
  eventPreventDefault,
  eventQueue,
  type Handler,
  type Policy,
} from "./policies.ts";

class RenderBoundary extends Component<
  { readonly broken: (cause: Cause.Cause<unknown>) => void; readonly children: ReactNode },
  { readonly failed: boolean }
> {
  override state = { failed: false };

  static getDerivedStateFromError(): { readonly failed: boolean } {
    return { failed: true };
  }

  override componentDidCatch(error: unknown): void {
    // Effect resumes synchronously; unmount only after React has finished this commit.
    queueMicrotask(() => this.props.broken(Cause.die(error)));
  }

  override render(): ReactNode {
    return this.state.failed ? null : this.props.children;
  }
}

/** The registries of the mounted roots, which a module replacement in development reads. */
const mounted = new Set<AtomRegistry.AtomRegistry>();

/** The states the next root starts from, kept across a module replacement. */
let carried: ReadonlyMap<string, unknown> | undefined;

hotHooks(
  () => {
    const states = new Map<string, unknown>();
    for (const registry of mounted) for (const [name, value] of statesOf(registry)) states.set(name, value);
    return states;
  },
  (states) => {
    carried = states;
  },
);

/**
 * Renders `ui` through a React renderer, once: the views in it render again
 * by themselves when the atoms they read change, reading them from
 * `registry`. Events run as tasks of the root under their policies; when
 * the fiber running the root is interrupted, events stop, React unmounts,
 * and the tasks are interrupted and joined.
 */
export const uiRunWith = <R>(
  renderer: Renderer,
  supervisor: (failure: Failure) => Effect.Effect<unknown, unknown, R>,
  ui: Ui,
  registry?: AtomRegistry.AtomRegistry,
): Effect.Effect<never, unknown, R> =>
  Effect.uninterruptibleMask((restore) =>
    Effect.gen(function* () {
      const context = yield* Effect.context<R>();
      let root: RenderRoot | undefined;
      let failure: Cause.Cause<unknown> | undefined;
      let resumeFailure: ((cause: Cause.Cause<unknown>) => void) | undefined;
      const broken = (cause: Cause.Cause<unknown>): void => {
        if (failure !== undefined) return;
        failure = cause;
        resumeFailure?.(cause);
      };
      // a task's end changes nothing the root draws: views follow their atoms
      // its handlers and instances reach the nodes of its elements
      const live = Context.add(context, DocumentService, document);
      const bridge = new Bridge(live, supervisor, () => undefined, broken);
      const run = Effect.gen(function* () {
        if (registry !== undefined) {
          if (carried !== undefined) seedStates(registry, carried);
          carried = undefined;
          mounted.add(registry);
        }
        root = yield* Effect.sync(renderer);
        try {
          const tree = createElement(BridgeContext.Provider, {
            value: bridge,
            children: createElement(RenderBoundary, { broken, children: toReact(bridge, ui) }),
          });
          root.render(registry === undefined ? tree : createElement(RegistryContext.Provider, { value: registry }, tree));
        } catch (error) {
          broken(Cause.die(error));
        }
        return yield* restore(
          Effect.callback<never, unknown>((resume) => {
            resumeFailure = (cause) => resume(Effect.failCause(cause));
            if (failure !== undefined) resumeFailure(failure);
          }),
        );
      });
      return yield* Effect.ensuring(
        run,
        Effect.gen(function* () {
          bridge.accepting = false;
          if (registry !== undefined) mounted.delete(registry);
          const unmounted = yield* Effect.exit(Effect.sync(() => root?.unmount()));
          yield* Fiber.interruptAll([...bridge.tasks]);
          yield* Fiber.awaitAll([...bridge.supervisors]);
          return yield* unmounted;
        }),
      );
    }),
  );

/**
 * `Ui.mount_with`: `uiRunWith` with the registry of the provided
 * `Reactivity.Atom.Registry`.
 */
export const uiMountWith = <R>(
  renderer: Renderer,
  supervisor: (failure: Failure) => Effect.Effect<unknown, unknown, R>,
  ui: Ui,
): Effect.Effect<never, unknown, unknown> =>
  Effect.flatMap(Effect.service(RegistryService), (record) => {
    const registry = nativeRegistry(record);
    if (registry === undefined) return Effect.die(new Error("`Reactivity.Atom.Registry` was not built by its `layer`"));
    return uiRunWith(renderer, supervisor, ui, registry);
  });
