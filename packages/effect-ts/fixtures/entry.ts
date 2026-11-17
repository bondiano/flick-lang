// Programs for the entry runner tests: `node entry.ts <case>`.
import { Effect, Option } from "effect";

import {
  type RunOptions,
  acquireRelease,
  appFail,
  applyRuntime,
  fiberSpawn,
  hostArguments,
  hostClock,
  hostConsole,
  hostEnvironment,
  hostRandom,
  provideHost,
  runMain,
  scope,
  service,
} from "../src/index.ts";

const say = (text: string) => Effect.sync(() => process.stdout.write(`${text}\n`));

/** `S.op args` as compiled code performs it. */
const perform = (key: string, op: string, args: ReadonlyArray<unknown>) =>
  Effect.flatMap(Effect.service(service(key)), (impl) => applyRuntime(Reflect.get(impl, op), args));

const fakeConsole = {
  stdout_line: (text: string) => say(`fake: ${text}`),
  stderr_line: (text: string) => say(`fake stderr: ${text}`),
  stdout_write: (text: string) => say(`fake write: ${text}`),
};

const provideFake = <R>(program: Effect.Effect<unknown, unknown, R>) =>
  Effect.provideService(program, service("Host.Console"), fakeConsole);

const hostRoot = () =>
  Effect.gen(function* () {
    yield* perform("Host.Console", "stdout_line", ["from the root"]);
    yield* perform("Host.Console", "stderr_line", ["to stderr"]);
    const variable = yield* perform("Host.Environment", "env", ["FLICK_HOST_ROOT"]);
    const args = yield* perform("Host.Arguments", "args", [undefined]);
    const millis = yield* perform("Host.Clock", "current_time_millis", [undefined]);
    const draw = yield* perform("Host.Random", "next", [undefined]);
    const shown = Option.isOption(variable) ? Option.getOrElse(variable, () => "none") : "not an option";
    yield* say([shown, JSON.stringify(args), typeof millis, typeof draw].join(" "));
  });

const resource = (release = say("released")) => acquireRelease(say("acquired"), () => release);

const waitForSignal = Effect.andThen(say("ready"), Effect.never);

// a program may need the services the runner provides
const programs: Readonly<Record<string, () => Effect.Effect<unknown, unknown, unknown>>> = {
  success: () => scope(() => Effect.andThen(resource(), say("done"))),
  "app-error": () => scope(() => Effect.andThen(resource(), appFail("config is missing"))),
  defect: () => scope(() => Effect.andThen(resource(), Effect.die(new Error("boom")))),
  signal: () => scope(() => Effect.andThen(resource(), waitForSignal)),
  "signal-failing-cleanup": () =>
    scope(() => Effect.andThen(resource(Effect.die(new Error("close failed"))), waitForSignal)),
  "slow-cleanup": () =>
    scope(() =>
      Effect.andThen(resource(Effect.andThen(Effect.sleep(200), say("released"))), waitForSignal),
    ),
  "supervisor-defect": () =>
    scope((owner) =>
      Effect.gen(function* () {
        yield* fiberSpawn(owner, () => Effect.die(new Error("supervisor broke")), Effect.fail("x"));
        yield* Effect.sleep(20);
      }),
    ),
  devtools: () => Effect.withSpan(Effect.andThen(Effect.logDebug("atom App.count"), say("done")), "App.go"),
  "host-root": hostRoot,
  "host-own": () => provideFake(perform("Host.Console", "stdout_line", ["mine"])),
  "host-provide": () => perform("Host.Console", "stdout_line", ["provided"]),
};

const name = process.argv[2] ?? "";
// as `flick run --devtools` preloads it
if (name === "devtools") await import("../src/runner/devtools.ts");
const program = programs[name] ?? (() => Effect.void);
// what a launcher passes for a main that names every Host service
const live = { provide: provideHost([hostConsole, hostClock, hostRandom, hostEnvironment, hostArguments]) };
const options: Readonly<Record<string, RunOptions>> = {
  "old-effect": { capabilities: ["the Flick Effect adapter requires effect 4.1.2, found 4.1.0"] },
  "host-root": live,
  "host-own": live,
  "host-provide": { provide: provideFake },
};
await runMain(program, options[name] ?? {});
