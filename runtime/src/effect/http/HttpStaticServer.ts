// Effect's `HttpStaticServer` for the bindings of the standard library, on
// Node's file system and platform.
import * as NodeHttpServer from "@effect/platform-node/NodeHttpServer";
import { Effect, Option } from "effect";
import * as StaticServer from "effect/http/HttpStaticServer";

import { toHandler } from "./handler.ts";
import { mapError as serverError } from "./HttpServerError.ts";
import { mapError } from "../PlatformError.ts";

export * from "effect/http/HttpStaticServer";

/** `Http.StaticServer.StaticOptions`. */
export interface StaticOptions {
  readonly root: string;
  readonly index: Option.Option<string>;
  readonly spa: boolean;
  readonly cacheControl: Option.Option<string>;
  readonly mimeTypes: ReadonlyArray<readonly [string, string]>;
}

/** `Http.StaticServer.make options`. */
export const flickMake = (o: StaticOptions) =>
  StaticServer.make({
    root: o.root,
    index: Option.getOrUndefined(o.index),
    spa: o.spa,
    cacheControl: Option.getOrUndefined(o.cacheControl),
    mimeTypes: Object.fromEntries(o.mimeTypes),
  }).pipe(
    Effect.map((app) => toHandler(serverError(app))),
    mapError,
    Effect.provide(NodeHttpServer.layerHttpServices),
  );
