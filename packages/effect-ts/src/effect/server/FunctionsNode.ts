// `Server.handler`: the implementations of server functions served as one
// RPC group, a POST per call in NDJSON, on a route of `Http.Router`. While
// it is mounted, a call made in this process runs in memory, as over HTTP:
// with the handler's services, the request's headers, and what crosses
// written and read by its codecs.
import { Effect, Exit, Option, Schema, Scope } from "effect";
import { HttpServerRequest } from "effect/http";
import * as RpcClient from "effect/rpc/RpcClient";
import * as RpcTest from "effect/rpc/RpcTest";

import { acquireRelease } from "../../owner.ts";
import { type Handler, prepare } from "../../rpc/handlers.ts";
import { type Group, type Procedure, rpcNdjson } from "../../rpc/model.ts";
import { rpcHandler } from "../../rpc/handler.ts";
import { codecDecode, codecEncode } from "../../schema/index.ts";
import type { AnySchema } from "../../schema/schemas.ts";

import { registerLocal, type ServerFunction } from "./Functions.ts";

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a flat client calls a procedure by its tag
const unary = (call: unknown) => call as (tag: string, payload: unknown) => Effect.Effect<unknown, unknown>;

/** `value` written and read back by `schema`, as it crosses the network. */
const across = (schema: AnySchema | null, value: unknown): Effect.Effect<unknown, never, unknown> =>
  schema === null
    ? Effect.succeed(value)
    : Effect.orDie(Effect.flatMap(codecEncode(schema, value), (encoded) => codecDecode(schema, encoded)));

/** A call of `procedure` through the in-memory `call`, with the headers of the request being served. */
const inMemory =
  (procedure: Procedure, call: (tag: string, payload: unknown) => Effect.Effect<unknown, unknown>) =>
  (payload: unknown): Effect.Effect<unknown, unknown, unknown> =>
    Effect.gen(function* () {
      const request = yield* Effect.serviceOption(HttpServerRequest.HttpServerRequest);
      const sent = yield* across(procedure.payload, payload);
      const run = call(procedure.name, sent);
      const result = yield* (Option.isSome(request) ? RpcClient.withHeaders(request.value.headers)(run) : run).pipe(
        Effect.catch((error) => {
          const codec = procedure.errors.find((e) => Schema.is(e)(error));
          return Effect.flatMap(codec === undefined ? Effect.succeed(error) : across(codec, error), Effect.fail);
        }),
      );
      return yield* across(procedure.success, result);
    });

/** The calls of `group` made in this process, run by `handlers` until the innermost scope closes. */
const inProcess = (group: Group, handlers: Readonly<Record<string, Handler>>) =>
  acquireRelease(
    Effect.gen(function* () {
      const { rpcs, scope, handled } = yield* prepare(group, handlers);
      const client = yield* RpcTest.makeClient(rpcs, { flatten: true }).pipe(
        Effect.provideContext(handled),
        Effect.provideService(Scope.Scope, scope),
      );
      const undo = group.procedures.map((p) => registerLocal(p.name, inMemory(p, unary(client))));
      return { scope, undo };
    }),
    ({ scope, undo }) => Effect.andThen(Effect.sync(() => undo.forEach((f) => f())), Scope.close(scope, Exit.void)),
  );

/** `Server.handler functions`, owned by the innermost scope. */
export const serverHandler = (functions: ReadonlyArray<ServerFunction>) => {
  const group: Group = { procedures: functions.map((f) => f.procedure) };
  const handlers = Object.fromEntries(functions.map((f) => [f.procedure.name, f.run]));
  return Effect.flatMap(inProcess(group, handlers), () => rpcHandler(rpcNdjson, group, handlers));
};
