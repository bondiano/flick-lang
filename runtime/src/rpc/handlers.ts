// The handlers of a group as Effect's server runs them: each with the
// services of the caller, its procedure's hooks first. Shared by the Node
// servers and the in-memory client, so it loads nothing of Node.
import { Effect, Scope, Stream } from "effect";
import type { Headers } from "effect/http";
import * as Rpc from "effect/rpc/Rpc";

import { effectGroup, type Group } from "./model.ts";

/** A handler of a procedure as Flick compiles it. */
export type Handler = (payload: unknown) => Effect.Effect<unknown, unknown, unknown>;

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a hook is a runtime function of the checker's typing
const toEffect = (value: unknown): Effect.Effect<unknown, unknown, unknown> => value as Effect.Effect<unknown, unknown, unknown>;

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker typed the handler's result as a stream
const toStream = (value: unknown): Stream.Stream<unknown, unknown> => value as Stream.Stream<unknown, unknown>;

/**
 * Effect's group of `group` with `handlers`, each running with the services
 * of the caller, in a scope of its own the server closes.
 */
export const prepare = (group: Group, handlers: Readonly<Record<string, Handler>>) =>
  Effect.gen(function* () {
    const context = yield* Effect.context<unknown>();
    const own: Record<string, (payload: unknown, call: { readonly headers: Headers.Headers }) => unknown> = {};
    for (const procedure of group.procedures) {
      const handler = handlers[procedure.name];
      if (handler === undefined) return yield* Effect.die(new TypeError(`no handler for ${procedure.name}`));
      const auth = procedure.auth;
      // the hooks of the procedure, then its check, then the handler with the service the check made for this
      // call alone, with the services of `listen`
      const handle = (payload: unknown, call: { readonly headers: Headers.Headers }) => {
        const seen = Object.freeze({ payload, headers: call.headers });
        const run =
          auth === undefined
            ? handler(payload)
            : Effect.flatMap(toEffect(auth.check(seen)), (impl) => Effect.provideService(handler(payload), auth.tag, impl));
        return Effect.provideContext(
          Effect.andThen(Effect.forEach(procedure.middleware, (hook) => toEffect(hook(seen)), { discard: true }), run),
          context,
        );
      };
      // a stream's handler gives the stream as its result; `uninterruptible` asks the server to run it so
      const run = procedure.stream
        ? (payload: unknown, call: { readonly headers: Headers.Headers }) => Stream.unwrap(Effect.map(handle(payload, call), toStream))
        : handle;
      own[procedure.name] = procedure.uninterruptible
        ? (payload: unknown, call: { readonly headers: Headers.Headers }) => Rpc.uninterruptible(run(payload, call))
        : run;
    }
    const rpcs = effectGroup(group.procedures);
    const scope = yield* Scope.make();
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker typed each handler by its procedure
    const handled = yield* Effect.provideService(rpcs.toHandlers(own as never), Scope.Scope, scope);
    return { rpcs, scope, handled };
  });

