// `Rpc.worker` and `Rpc.serve_worker`: calls to a Web Worker and their
// answers inside it, over structured clone rather than a serialization.
// The Node threads are `./server.ts`, so a browser bundle loads no Node.
import * as BrowserWorker from "@effect/platform-browser/BrowserWorker";
import * as BrowserWorkerRunner from "@effect/platform-browser/BrowserWorkerRunner";
import { Effect, Layer } from "effect";
import * as RpcClient from "effect/rpc/RpcClient";
import * as RpcServer from "effect/rpc/RpcServer";

import type { Client } from "./client.ts";
import { type Handler, serveOn } from "./handlers.ts";
import { clientError, type Group } from "./model.ts";

/**
 * `Rpc.worker url`: a connection to a module worker started from `url`,
 * whose `serve_worker` answers; the scope's end closes it.
 */
export const rpcWorker = (url: string) =>
  Effect.map(
    Layer.build(
      // shortcut: one worker per client; a pool size becomes a parameter when a program needs one
      RpcClient.layerProtocolWorker({ size: 1 }).pipe(Layer.provide(BrowserWorker.layer(() => new Worker(url, { type: "module" })))),
    ),
    (context): Client => ({ context }),
  ).pipe(Effect.mapError((error) => clientError(`cannot start ${url}: ${error.message}`)));

/**
 * `Rpc.serve_worker group handlers`: the calls of the page that started this
 * worker, answered until it closes the worker.
 */
export const rpcServeWorker = (group: Group, handlers: Readonly<Record<string, Handler>>) =>
  serveOn(RpcServer.layerProtocolWorkerRunner.pipe(Layer.provide(BrowserWorkerRunner.layer), Layer.orDie), group, handlers);
