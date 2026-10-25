// `Runner.Cloudflare`: a Worker whose `fetch` answers each request with
// `main`, run under its own root owner on `{ request, env, context }`; a
// failure is reported on the console and answered with 500.
import { Effect, Exit } from "effect";
import * as ServerRequest from "effect/http/HttpServerRequest";
import * as ServerResponse from "effect/http/HttpServerResponse";

import { asMain, conclude, pretty, start } from "../entry.ts";
import { writeStderr } from "../host.ts";
import { reportedDefects } from "../owner.ts";

/** What a Worker's runtime gives `fetch` besides the request. */
interface Context {
  waitUntil(promise: Promise<unknown>): void;
}

type Env = Readonly<Record<string, unknown>>;

/** What `main` takes: `Runner.Cloudflare.Fetch`. */
interface Fetch {
  readonly request: ServerRequest.HttpServerRequest;
  readonly env: Env;
  readonly context: Context;
}

const failed = (): Response => new Response("Internal Server Error", { status: 500 });

export const run = (main: unknown) => {
  const handle = asMain<Fetch>(main);
  return {
    fetch: async (request: Request, env: Env = {}, context: Context = { waitUntil: () => {} }): Promise<Response> => {
      const fiber = start(handle, { request: ServerRequest.fromWeb(request), env, context });
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

export const envVar = (name: string, env: Env): string | undefined => {
  const value = env[name];
  return typeof value === "string" ? value : undefined;
};

export const envBinding = (name: string, env: Env): unknown => env[name];

/** `wait_until`: the task runs on its own, as the response no longer waits for it. */
export const waitUntil = (context: Context, task: Effect.Effect<unknown>): Effect.Effect<void> =>
  Effect.sync(() =>
    context.waitUntil(
      Effect.runPromise(
        Effect.catchCause(task, (cause) => Effect.sync(() => writeStderr(`error: ${pretty(cause)}\n`))),
      ),
    ),
  );
