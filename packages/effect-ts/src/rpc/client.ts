// `Rpc.call` and `Rpc.call_stream`: a client of one procedure over fetch or
// the global WebSocket, which run in browsers as on Node; `Rpc.connect`
// keeps one connection for many calls. Each transport is a value of its
// own, so a program bundles only those it names.
import { type Context, Effect, Layer, Predicate, Stream } from "effect";
import { FetchHttpClient } from "effect/http";
import * as RpcClient from "effect/rpc/RpcClient";
import * as Socket from "effect/socket/Socket";
import { type CallOptions, clientError, effectGroup, type Procedure, rpcNdjson, type Transport } from "./model.ts";

/** A failure of the client as a `ClientError`; a typed error of the procedure stays as it is. */
const clientFailure = (error: unknown): unknown =>
  Predicate.isTagged(error, "RpcClientError") ? clientError(error instanceof Error ? error.message : "the RPC client failed") : error;

/** `Rpc.http`: a POST per call over fetch. */
export const rpcHttp: Transport = (url, serialization) =>
  RpcClient.layerProtocolHttp({ url }).pipe(Layer.provide([FetchHttpClient.layer, serialization]));

/** `Rpc.websocket`: the messages of calls over the global WebSocket. */
export const rpcWebsocket: Transport = (url, serialization) =>
  RpcClient.layerProtocolSocket().pipe(
    Layer.provide(Socket.layerWebSocket(url)),
    Layer.provide([Socket.layerWebSocketConstructorGlobal, serialization]),
  );

const protocol = (options: CallOptions, url: string): Layer.Layer<RpcClient.Protocol> => options.transport(url, options.serialization);

/** A client of `procedure` over the protocol in `context`. */
const over = (context: Context.Context<RpcClient.Protocol>, procedure: Procedure) =>
  Effect.provideContext(RpcClient.make(effectGroup([procedure]), { flatten: true }), context);

// the protocol lives as long as the call's scope: a socket must stay open after the client is made
const client = (options: CallOptions, url: string, procedure: Procedure) =>
  Effect.flatMap(Layer.build(protocol(options, url)), (context) => over(context, procedure));

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a flat client calls a procedure by its tag
const unary = (call: unknown) => call as (tag: string, payload: unknown) => Effect.Effect<unknown, unknown>;

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a flat client calls a stream's procedure by its tag
const streamed = (call: unknown) => call as (tag: string, payload: unknown) => Stream.Stream<unknown, unknown>;

/** `Rpc.call_with options url procedure payload`. */
export const rpcCall = (options: CallOptions, url: string, procedure: Procedure, payload: unknown): Effect.Effect<unknown, unknown> =>
  Effect.scoped(
    Effect.flatMap(client(options, url, procedure), (call) =>
      Effect.mapError(unary(call)(procedure.name, payload), clientFailure),
    ),
  );

/** `Rpc.call_stream_with options url procedure payload`. */
export const rpcCallStream = (options: CallOptions, url: string, procedure: Procedure, payload: unknown): Stream.Stream<unknown, unknown> =>
  Stream.unwrap(
    Effect.map(client(options, url, procedure), (call) =>
      Stream.mapError(streamed(call)(procedure.name, payload), clientFailure),
    ),
  ).pipe(Stream.scoped);

const ndjson: CallOptions = { serialization: rpcNdjson, transport: rpcHttp };

/** `Rpc.call url procedure payload`: in NDJSON. */
export const rpcCallNdjson = (url: string, procedure: Procedure, payload: unknown): Effect.Effect<unknown, unknown> =>
  rpcCall(ndjson, url, procedure, payload);

/** `Rpc.call_stream url procedure payload`: in NDJSON. */
export const rpcCallStreamNdjson = (url: string, procedure: Procedure, payload: unknown): Stream.Stream<unknown, unknown> =>
  rpcCallStream(ndjson, url, procedure, payload);

/** `Rpc.Client s`: a connection that calls share until its scope closes. */
export interface Client {
  readonly context: Context.Context<RpcClient.Protocol>;
}

/** `Rpc.connect options url`: the protocol, built in the innermost scope. */
export const rpcConnect = (options: CallOptions, url: string) =>
  Effect.map(Layer.build(protocol(options, url)), (context): Client => ({ context }));

/** `Rpc.call_on client procedure payload`. */
export const rpcCallOn = (connection: Client, procedure: Procedure, payload: unknown): Effect.Effect<unknown, unknown> =>
  Effect.scoped(Effect.flatMap(over(connection.context, procedure), (call) => Effect.mapError(unary(call)(procedure.name, payload), clientFailure)));

/** `Rpc.call_stream_on client procedure payload`. */
export const rpcCallStreamOn = (connection: Client, procedure: Procedure, payload: unknown): Stream.Stream<unknown, unknown> =>
  Stream.unwrap(Effect.map(over(connection.context, procedure), (call) => Stream.mapError(streamed(call)(procedure.name, payload), clientFailure))).pipe(
    Stream.scoped,
  );

/** `Rpc.with_headers input task`: the calls of `task` carry the headers too. */
export const rpcWithHeaders =
  (input: ReadonlyArray<readonly [string, string]>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>): Effect.Effect<A, E, R> =>
    RpcClient.withHeaders(self, input);
