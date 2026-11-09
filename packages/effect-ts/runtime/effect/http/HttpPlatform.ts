// Effect's `HttpPlatform` for the bindings of the standard library: the
// Flick service `Http.Platform.HttpPlatform` over Node's platform.
import * as NodeHttpServer from "@effect/platform-node/NodeHttpServer";
import { type Context, Effect, Layer, Option } from "effect";
import type * as ByteSize from "effect/ByteSize";
import * as HttpPlatform from "effect/http/HttpPlatform";

import { enumeration } from "./convert.ts";
import { type FlickOptions, options } from "./HttpServerResponse.ts";
import { mapError } from "../PlatformError.ts";

export * from "effect/http/HttpPlatform";

/** `Http.Platform.CompressionAlgorithm`. */
export const compression = /*#__PURE__*/ enumeration<HttpPlatform.CompressionAlgorithm>("Http.Platform.CompressionAlgorithm", [
  ["Gzip", "gzip"],
  ["Deflate", "deflate"],
  ["Brotli", "br"],
  ["Zstd", "zstd"],
]);

/** `Http.Platform.FileOptions`. */
export interface FileOptions {
  readonly bytesToRead: Option.Option<ByteSize.ByteSize>;
  readonly chunkSize: Option.Option<number>;
  readonly offset: Option.Option<ByteSize.ByteSize>;
  readonly response: FlickOptions;
}

const fileOptions = (o: FileOptions) => ({
  ...options(o.response),
  bytesToRead: Option.getOrUndefined(o.bytesToRead),
  chunkSize: Option.getOrUndefined(o.chunkSize),
  offset: Option.getOrUndefined(o.offset),
});

const impl = (platform: HttpPlatform.HttpPlatform["Service"]) => ({
  file_response: (o: FileOptions) => (path: string) => mapError(platform.fileResponse(path, fileOptions(o))),
});

type Impl = ReturnType<typeof impl>;

/** `Http.Platform.node`. */
export const flickNode = <I>(key: Context.Key<I, Impl>): Layer.Layer<I> =>
  Layer.effect(key, Effect.map(Effect.service(HttpPlatform.HttpPlatform), impl)).pipe(
    Layer.provide(NodeHttpServer.layerHttpServices),
  );
