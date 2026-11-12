// `Http.Server` on Flick's own runtime: `serve` listens on Node.js for the
// owner of the caller's scope (`../../http/node.ts` has the lifecycle), and
// answers a handler's failure as the adapter does. A `Server` is where it
// listens. Logging the address is the adapter's until Flick's loggers run
// here.
import type { Option } from "../../../core/data.ts";
import { tag } from "../../../tags.ts";
import { type Duration, durationHooks } from "../../duration.ts";
import type { Run } from "../../fiber.ts";
import type { ServerRequest } from "../../http/model.ts";
import { serveError } from "../../http/model.ts";
import { type App, type Server, ownedServer } from "../../http/node.ts";

/** `Http.Server.ServeOptions`. */
export interface ServeOptions {
  readonly host: string;
  readonly port: number;
  readonly gracefulShutdownTimeout: Option<Duration>;
}

const DEFAULT_GRACE_MILLIS = 20_000;

/** `Http.Server.serve options handler`. */
export const flickServe = (options: ServeOptions, handler: (request: ServerRequest) => unknown): Run =>
  ownedServer(
    {
      host: options.host,
      port: options.port,
      graceMillis:
        options.gracefulShutdownTimeout._tag === "Some" ? durationHooks.millis(options.gracefulShutdownTimeout.value) : DEFAULT_GRACE_MILLIS,
    },
    handler satisfies App,
    serveError,
  );

const TCP = /*#__PURE__*/ tag("Http.Server.Address.TcpAddress");

/** `Http.Server.address server`. */
export const flickAddress = (server: Server) => ({ _tag: TCP, _0: server.host, _1: server.port });
