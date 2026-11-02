// `HttpApi.Inject.call`: `HttpApi.call` whose request goes to a handler of
// the server instead of the network, as Fastify's `inject` does, so a test
// runs the whole server, routing, decoding and errors, without a socket.
import { Effect } from "effect";
import * as HttpServerRequest from "effect/http/HttpServerRequest";
import * as HttpServerResponse from "effect/http/HttpServerResponse";

import { causeResponse } from "../effect/http/HttpServerResponse.ts";
import { type Request, type Send, callWith } from "./client.ts";
import type { Endpoint } from "./model.ts";

type Handler = (
  request: HttpServerRequest.HttpServerRequest,
) => Effect.Effect<HttpServerResponse.HttpServerResponse, unknown, unknown>;

/** The request as the server gets it, answered by `handler`. */
const injected =
  (handler: Handler): Send =>
  (url, init) =>
    Effect.gen(function* () {
      // an error the handler did not answer is what a server would send
      const answered = yield* Effect.catchCause(handler(HttpServerRequest.fromWeb(new Request(url, init))), causeResponse);
      const response = HttpServerResponse.toWeb(answered);
      const bytes = new Uint8Array(yield* Effect.promise(() => response.arrayBuffer()));
      return { status: response.status, headers: response.headers, bytes };
    });

/** `HttpApi.Inject.call handler base endpoint request`. */
export const httpApiInject = (handler: Handler, base: string, endpoint: Endpoint, request: Request) =>
  callWith(injected(handler), new URL(base.endsWith("/") ? base : `${base}/`, "http://localhost/"), endpoint, request);
