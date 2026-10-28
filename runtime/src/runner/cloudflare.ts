// `Runner.Cloudflare`: a Worker whose `fetch` answers each request with
// `main`, run under its own root owner on `{ request, context }` with the
// bindings as the service `Env`; a failure is reported on the console and
// answered with 500.
import { Effect, Exit, Option } from "effect";
import * as ServerRequest from "effect/http/HttpServerRequest";
import * as ServerResponse from "effect/http/HttpServerResponse";

import { service } from "../effects.ts";
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
  readonly context: Context;
}

/** The service `Runner.Cloudflare.Env` over a request's bindings. */
const EnvService = service("Runner.Cloudflare.Env");

const bindings = (env: Env) => ({
  var: (name: string) => {
    const value = env[name];
    return Effect.succeed(typeof value === "string" ? Option.some(value) : Option.none());
  },
  binding: (name: string) => Effect.succeed(Option.fromNullishOr(env[name])),
});

const failed = (): Response => new Response("Internal Server Error", { status: 500 });

export const run = (main: unknown) => {
  const handle = asMain<Fetch>(main);
  return {
    fetch: async (request: Request, env: Env = {}, context: Context = { waitUntil: () => {} }): Promise<Response> => {
      const fiber = start(handle, { request: ServerRequest.fromWeb(request), context }, Effect.provideService(EnvService, bindings(env)));
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

/** `wait_until`: the task runs on its own, as the response no longer waits for it. */
export const waitUntil = (context: Context, task: Effect.Effect<unknown>): Effect.Effect<void> =>
  Effect.sync(() =>
    context.waitUntil(
      Effect.runPromise(
        Effect.catchCause(task, (cause) => Effect.sync(() => writeStderr(`error: ${pretty(cause)}\n`))),
      ),
    ),
  );
