// `Rpc.call` and `Rpc.call_stream`: a client of one procedure over fetch,
// which runs in browsers as on Node.
import { Effect, Layer, Predicate, Stream } from "effect";
import { FetchHttpClient } from "effect/http";
import * as RpcClient from "effect/rpc/RpcClient";
import { type CallOptions, clientError, effectGroup, type Procedure, rpcNdjson } from "./model.ts";

/** A failure of the client as a `ClientError`; a typed error of the procedure stays as it is. */
const clientFailure = (error: unknown): unknown =>
  Predicate.isTagged(error, "RpcClientError") ? clientError(error instanceof Error ? error.message : "the RPC client failed") : error;

/** What a client at `url` needs: the protocol over fetch with the serialization of `options`. */
const protocol = (options: CallOptions, url: string) =>
  RpcClient.layerProtocolHttp({ url }).pipe(Layer.provide([FetchHttpClient.layer, options.serialization]));

const client = (options: CallOptions, url: string, procedure: Procedure) =>
  Effect.provide(RpcClient.make(effectGroup([procedure]), { flatten: true }), protocol(options, url));

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

const ndjson: CallOptions = { serialization: rpcNdjson };

/** `Rpc.call url procedure payload`: in NDJSON. */
export const rpcCallNdjson = (url: string, procedure: Procedure, payload: unknown): Effect.Effect<unknown, unknown> =>
  rpcCall(ndjson, url, procedure, payload);

/** `Rpc.call_stream url procedure payload`: in NDJSON. */
export const rpcCallStreamNdjson = (url: string, procedure: Procedure, payload: unknown): Stream.Stream<unknown, unknown> =>
  rpcCallStream(ndjson, url, procedure, payload);
