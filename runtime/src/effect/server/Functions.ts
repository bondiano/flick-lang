// The calls of `server fn`: the compiler makes a procedure of each from its
// name and the witnesses of what it sends; a caller sends it over HTTP and
// `Server.handler` serves it with its implementation. Serving is
// `./FunctionsNode.ts`, which needs Node.
import { Effect } from "effect";

import { env } from "../../host.ts";
import { rpcCallNdjson } from "../../rpc/client.ts";
import {
  clientError,
  type Procedure,
  rpcMake,
  rpcWithErrors,
  rpcWithPayload,
  rpcWithSuccess,
} from "../../rpc/model.ts";
import { schemaJsonCodec } from "../../schema/index.ts";
import type { AnySchema } from "../../schema/schemas.ts";
import type { Handler } from "../../rpc/handlers.ts";

/** `Server.path`. */
const PATH = "/_flick/fn";

/** `Server.Function r`: an implementation with the procedure of its calls. */
export interface ServerFunction {
  readonly procedure: Procedure;
  readonly run: Handler;
}

/** `Server.procedure name payload success errors`. */
export const serverProcedure = (
  name: string,
  payload: AnySchema,
  success: AnySchema,
  errors: ReadonlyArray<AnySchema>,
): Procedure =>
  rpcWithErrors(
    errors.map(schemaJsonCodec),
    rpcWithSuccess(schemaJsonCodec(success), rpcWithPayload(schemaJsonCodec(payload), rpcMake(name))),
  );

/** Where the calls go: the page's origin in a browser, `FLICK_SERVER_URL` elsewhere. */
const base = (): string | undefined => {
  const location: unknown = Reflect.get(globalThis, "location");
  const origin: unknown = typeof location === "object" && location !== null ? Reflect.get(location, "origin") : undefined;
  return typeof origin === "string" && origin !== "null" ? origin : env("FLICK_SERVER_URL");
};

/** `Server.call procedure payload`. */
export const serverCall = (procedure: Procedure, payload: unknown): Effect.Effect<unknown, unknown> =>
  Effect.suspend(() => {
    const url = base();
    return url === undefined
      ? Effect.fail(clientError(`no server for ${procedure.name}: set FLICK_SERVER_URL`))
      : rpcCallNdjson(new URL(PATH, url).href, procedure, payload);
  });

/** `Server.function procedure run`. */
export const serverFunction = (procedure: Procedure, run: Handler): ServerFunction =>
  Object.freeze({ procedure, run });
