// Effect's `effect/rpc` for the standard library, and Flick's typed RPCs:
// procedures and groups as data, served over HTTP with NDJSON for as long as
// the Flick scope that started it, called with a client of the same
// procedures. The codecs are Flick's (Effect Schema codecs of the JSON wire
// form), so they are the RPC schemas as they are.
import * as NodeHttpServer from "@effect/platform-node/NodeHttpServer";
import { Effect, Exit, Layer, Predicate, Schema, Scope, Stream } from "effect";
import { FetchHttpClient, HttpRouter } from "effect/http";
import * as Rpc from "effect/rpc/Rpc";
import * as RpcClient from "effect/rpc/RpcClient";
import * as RpcGroup from "effect/rpc/RpcGroup";
import * as RpcSerialization from "effect/rpc/RpcSerialization";
import * as RpcServer from "effect/rpc/RpcServer";

import type { Tagged } from "../../data.ts";
import { nodeHttp } from "../../host.ts";
import { acquireRelease } from "../../owner.ts";
import type { AnySchema } from "../../schema/schemas.ts";
import { tag } from "../../tags.ts";

export * from "effect/rpc/Rpc";

/** `Rpc.Rpc l f`: a procedure as data, made into Effect's when served or called. */
export interface Procedure {
  readonly name: string;
  readonly payload: AnySchema | null;
  readonly success: AnySchema | null;
  readonly errors: ReadonlyArray<AnySchema>;
  readonly stream: boolean;
}

/** `Rpc.Group hs`. */
export interface Group {
  readonly procedures: ReadonlyArray<Procedure>;
}

/** `Rpc.ServeOptions`. */
export interface ServeOptions {
  readonly host: string;
  readonly port: number;
  readonly path: string;
}

/** `Rpc.Server s`. */
export interface Server {
  readonly port: number;
  readonly scope: Scope.Closeable;
}

type Handler = (payload: unknown) => Effect.Effect<unknown, unknown, unknown>;

/** `Rpc.ServeError` and `Rpc.ClientError`. */
export type RpcError = Tagged<string, string>;

const serveError = (message: string): RpcError => ({ _tag: tag("Rpc.ServeError.ServeError"), _0: message });
const clientError = (message: string): RpcError => ({ _tag: tag("Rpc.ClientError.ClientError"), _0: message });

const with_ = (procedure: Procedure, change: Partial<Procedure>): Procedure => Object.freeze({ ...procedure, ...change });

/** `Rpc.make label`: a procedure without payload, success or errors. */
export const rpcMake = (name: string): Procedure =>
  Object.freeze({ name, payload: null, success: null, errors: [], stream: false });

export const rpcWithPayload = (codec: AnySchema, procedure: Procedure): Procedure => with_(procedure, { payload: codec });

export const rpcWithSuccess = (codec: AnySchema, procedure: Procedure): Procedure =>
  with_(procedure, { success: codec, stream: false });

export const rpcWithStream = (codec: AnySchema, procedure: Procedure): Procedure =>
  with_(procedure, { success: codec, stream: true });

/** `Rpc.error_case codec`: the codec is the case. */
export const rpcErrorCase = (codec: AnySchema): AnySchema => codec;

export const rpcWithErrors = (cases: ReadonlyArray<AnySchema>, procedure: Procedure): Procedure =>
  with_(procedure, { errors: [...procedure.errors, ...cases] });

/** `Rpc.group`: a group without procedures. */
export const rpcGroup: Group = /*#__PURE__*/ Object.freeze({ procedures: [] });

/** `Rpc.add`, after the dictionaries of its record constraints. */
export const rpcAdd = (_lacks: unknown, _union: unknown, procedure: Procedure, group: Group): Group =>
  Object.freeze({ procedures: [...group.procedures, procedure] });

/** Effect's `Rpc` of `procedure`. */
const effectRpc = (procedure: Procedure) => {
  const [first, ...rest] = procedure.errors;
  const error = first === undefined ? Schema.Never : rest.length === 0 ? first : Schema.Union([first, ...rest]);
  return Rpc.make(procedure.name, {
    payload: procedure.payload ?? Schema.Void,
    success: procedure.success ?? Schema.Void,
    error,
    stream: procedure.stream,
  });
};

const effectGroup = (procedures: ReadonlyArray<Procedure>) => RpcGroup.make(...procedures.map(effectRpc));

const isRoutePath = (path: string): path is `/${string}` => path.startsWith("/");

const start = (options: ServeOptions, group: Group, handlers: Readonly<Record<string, Handler>>) =>
  Effect.gen(function* () {
    const context = yield* Effect.context<unknown>();
    const own: Record<string, (payload: unknown) => unknown> = {};
    for (const procedure of group.procedures) {
      const handler = handlers[procedure.name];
      if (handler === undefined) return yield* Effect.die(new TypeError(`no handler for ${procedure.name}`));
      // a handler runs with the services of `listen`; a stream's handler gives the stream as its result
      own[procedure.name] = procedure.stream
        ? (payload) => Stream.unwrap(Effect.provideContext(Effect.map(handler(payload), toStream), context))
        : (payload) => Effect.provideContext(handler(payload), context);
    }
    const rpcs = effectGroup(group.procedures);
    const scope = yield* Scope.make();
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker typed each handler by its procedure
    const handled = yield* Effect.provideService(rpcs.toHandlers(own as never), Scope.Scope, scope);
    const app = yield* RpcServer.toHttpEffect(rpcs).pipe(
      Effect.provideContext(handled),
      Effect.provide(RpcSerialization.layerNdjson),
      Effect.provideService(Scope.Scope, scope),
    );
    const router = yield* HttpRouter.make;
    yield* router.add("POST", isRoutePath(options.path) ? options.path : `/${options.path}`, app);
    const made = yield* NodeHttpServer.make(() => nodeHttp().createServer(), { host: options.host, port: options.port }).pipe(
      Effect.provideService(Scope.Scope, scope),
      Effect.mapError((error) => serveError(error.message)),
      Effect.onError((cause) => Scope.close(scope, Exit.failCause(cause))),
    );
    yield* made.serve(router.asHttpEffect()).pipe(Effect.provideService(Scope.Scope, scope));
    const port = made.address._tag === "UnixPathAddress" ? 0 : made.address.port;
    return { port, scope } satisfies Server;
  });

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker typed the handler's result as a stream
const toStream = (value: unknown): Stream.Stream<unknown, unknown> => value as Stream.Stream<unknown, unknown>;

/** `Rpc.listen options group handlers`, owned by the innermost scope. */
export const rpcListen = (
  options: ServeOptions,
  group: Group,
  handlers: Readonly<Record<string, Handler>>,
): Effect.Effect<Server, RpcError, unknown> => acquireRelease(start(options, group, handlers), (server) => Scope.close(server.scope, Exit.void));

export const rpcPort = (server: Server): number => server.port;

/** A failure of the client as a `ClientError`; a typed error of the procedure stays as it is. */
const clientFailure = (error: unknown): unknown =>
  Predicate.isTagged(error, "RpcClientError") ? clientError(error instanceof Error ? error.message : "the RPC client failed") : error;

/** What a client of `procedure` at `url` needs: the protocol over fetch with NDJSON. */
const protocol = (url: string) =>
  RpcClient.layerProtocolHttp({ url }).pipe(Layer.provide([FetchHttpClient.layer, RpcSerialization.layerNdjson]));

const client = (url: string, procedure: Procedure) =>
  Effect.provide(RpcClient.make(effectGroup([procedure]), { flatten: true }), protocol(url));

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a flat client calls a procedure by its tag
const unary = (call: unknown) => call as (tag: string, payload: unknown) => Effect.Effect<unknown, unknown>;

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a flat client calls a stream's procedure by its tag
const streamed = (call: unknown) => call as (tag: string, payload: unknown) => Stream.Stream<unknown, unknown>;

/** `Rpc.call url procedure payload`. */
export const rpcCall = (url: string, procedure: Procedure, payload: unknown): Effect.Effect<unknown, unknown> =>
  Effect.scoped(
    Effect.flatMap(client(url, procedure), (call) =>
      Effect.mapError(unary(call)(procedure.name, payload), clientFailure),
    ),
  );

/** `Rpc.call_stream url procedure payload`. */
export const rpcCallStream = (url: string, procedure: Procedure, payload: unknown): Stream.Stream<unknown, unknown> =>
  Stream.unwrap(
    Effect.map(client(url, procedure), (call) =>
      Stream.mapError(streamed(call)(procedure.name, payload), clientFailure),
    ),
  ).pipe(Stream.scoped);
