// Programs for the entry runner tests: `node entry.ts <case>`.
import { Effect } from "effect";

import { acquireRelease, appFail, fiberSpawn, runMain, scope } from "../src/index.ts";

const say = (text: string) => Effect.sync(() => process.stdout.write(`${text}\n`));

const resource = (release = say("released")) => acquireRelease(say("acquired"), () => release);

const waitForSignal = Effect.andThen(say("ready"), Effect.never);

const programs: Readonly<Record<string, () => Effect.Effect<unknown, unknown>>> = {
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
};

const name = process.argv[2] ?? "";
// as `flick run --devtools` preloads it
if (name === "devtools") await import("../src/runner/devtools.ts");
const program = programs[name] ?? (() => Effect.void);
await runMain(
  program,
  name === "old-effect" ? { capabilities: ["the Flick Effect adapter requires effect 4.1.2, found 4.1.0"] } : {},
);
