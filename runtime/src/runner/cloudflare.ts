// `Runner.Cloudflare`: a Worker whose `fetch` answers each request with
// `main`, run under its own root owner; a failure is reported on the
// console and answered with 500.
import { Exit } from "effect";
import * as ServerRequest from "effect/http/HttpServerRequest";
import * as ServerResponse from "effect/http/HttpServerResponse";

import { asMain, conclude, start } from "../entry.ts";
import { writeStderr } from "../host.ts";
import { reportedDefects } from "../owner.ts";

const failed = (): Response => new Response("Internal Server Error", { status: 500 });

// shortcut: the Worker's `env` and `ctx` are not passed on; a service of the request is for when a program needs them
export const run = (main: unknown) => {
  const handle = asMain<ServerRequest.HttpServerRequest>(main);
  return {
    fetch: async (request: Request): Promise<Response> => {
      const fiber = start(handle, ServerRequest.fromWeb(request));
      const exit = await new Promise<Exit.Exit<unknown, unknown>>((resolve) => fiber.addObserver(resolve));
      if (Exit.isFailure(exit)) {
        conclude(exit, undefined, reportedDefects());
        return failed();
      }
      if (!ServerResponse.isHttpServerResponse(exit.value)) {
        writeStderr("error: `main` did not answer with a `ServerResponse`\n");
        return failed();
      }
      return ServerResponse.toWeb(exit.value);
    },
  };
};
