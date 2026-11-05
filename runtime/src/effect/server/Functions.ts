// The calls of `server fn`: std's plugin makes a procedure of each from its
// name and the witnesses of what it sends; a caller sends it over HTTP and
// `Server.handler` serves it with its implementation; while a handler is
// mounted in this process, a call made here (rendering a page on the
// server) runs in memory instead. Serving is `./FunctionsNode.ts`, which
// needs Node.
import { type Context, Effect } from "effect";

import { env } from "../../host.ts";
import { rpcCallNdjson } from "../../rpc/client.ts";
import {
  clientError,
  type Procedure,
  rpcMake,
  rpcWithAuth,
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

/** `Server.Derive.procedure name payload success errors`. */
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

/** `Server.Derive.no_errors`. */
export const serverNoErrors: ReadonlyArray<AnySchema> = Object.freeze([]);

/** `Server.Derive.error witness errors`. */
export const serverError = (witness: AnySchema, errors: ReadonlyArray<AnySchema>): ReadonlyArray<AnySchema> =>
  Object.freeze([witness, ...errors]);

/** Where the calls go: the page's origin in a browser, `FLICK_SERVER_URL` elsewhere. */
const base = (): string | undefined => {
  const location: unknown = Reflect.get(globalThis, "location");
  const origin: unknown = typeof location === "object" && location !== null ? Reflect.get(location, "origin") : undefined;
  return typeof origin === "string" && origin !== "null" ? origin : env("FLICK_SERVER_URL");
};

/** A call of a procedure made in memory. */
type Local = (payload: unknown) => Effect.Effect<unknown, unknown, unknown>;

// shortcut: one table per process, the last `Server.handler` mounted wins; key it by server once a process serves several
const local = new Map<string, Local>();

/** Runs the calls of `name` made in this process with `call`, until the returned function undoes it. */
export const registerLocal = (name: string, call: Local): (() => void) => {
  local.set(name, call);
  return () => {
    if (local.get(name) === call) local.delete(name);
  };
};

/** `Server.Derive.call procedure payload`. */
export const serverCall = (procedure: Procedure, payload: unknown): Effect.Effect<unknown, unknown, unknown> =>
  Effect.suspend(() => {
    const here = local.get(procedure.name);
    if (here !== undefined) return here(payload);
    const url = base();
    return url === undefined
      ? Effect.fail(clientError(`no server for ${procedure.name}: set FLICK_SERVER_URL`))
      : rpcCallNdjson(new URL(PATH, url).href, procedure, payload);
  });

/** `Server.authenticated tag check functions`: each run with the service `check` makes from its call's headers. */
export const serverAuthenticated = (
  tag: Context.Key<unknown, unknown>,
  check: (headers: unknown) => unknown,
  functions: ReadonlyArray<ServerFunction>,
): ReadonlyArray<ServerFunction> =>
  functions.map((f) =>
    serverFunction(rpcWithAuth(tag, (call) => check(Reflect.get(Object(call), "headers")), f.procedure), f.run),
  );

/** `Server.Derive.function procedure run` and `guarded`. */
export const serverFunction = (procedure: Procedure, run: Handler): ServerFunction =>
  Object.freeze({ procedure, run });
