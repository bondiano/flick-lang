// `Rpc.call_local`: a group called in memory, its client and server joined
// without a transport or a serialization, for tests.
import { Effect, Scope, Stream, Exit } from "effect";
import * as RpcTest from "effect/rpc/RpcTest";

import { type Handler, prepare } from "./handlers.ts";
import type { Group, Procedure } from "./model.ts";

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a flat client calls a procedure by its tag
const unary = (call: unknown) => call as (tag: string, payload: unknown) => Effect.Effect<unknown, unknown>;

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a flat client calls a stream's procedure by its tag
const streamed = (call: unknown) => call as (tag: string, payload: unknown) => Stream.Stream<unknown, unknown>;

/** The flat client of `group` with `handlers`, the handlers' scope closed with the caller's. */
const local = (group: Group, handlers: Readonly<Record<string, Handler>>) =>
  Effect.gen(function* () {
    const { rpcs, scope, handled } = yield* prepare(group, handlers);
    yield* Effect.addFinalizer(() => Scope.close(scope, Exit.void));
    return yield* Effect.provideContext(RpcTest.makeClient(rpcs, { flatten: true }), handled);
  });

/** `Rpc.call_local group handlers procedure payload`. */
export const rpcCallLocal = (group: Group, handlers: Readonly<Record<string, Handler>>, procedure: Procedure, payload: unknown) =>
  Effect.scoped(Effect.flatMap(local(group, handlers), (call) => unary(call)(procedure.name, payload)));

/** `Rpc.call_stream_local group handlers procedure payload`. */
export const rpcCallStreamLocal = (group: Group, handlers: Readonly<Record<string, Handler>>, procedure: Procedure, payload: unknown) =>
  Stream.scoped(Stream.unwrap(Effect.map(local(group, handlers), (call) => streamed(call)(procedure.name, payload))));
