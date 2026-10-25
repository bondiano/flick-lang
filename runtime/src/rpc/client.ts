// `Rpc.call` and `Rpc.call_stream`: a client of one procedure over fetch,
// which runs in browsers as on Node.
import { Effect, Layer, Predicate, Stream } from "effect";
import { FetchHttpClient } from "effect/http";
import * as RpcClient from "effect/rpc/RpcClient";
import * as RpcSerialization from "effect/rpc/RpcSerialization";

import { clientError, effectGroup, type Procedure } from "./model.ts";

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
