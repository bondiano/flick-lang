// Programs for the entry runner tests: `node entry.mjs <case>`.
import { Effect } from "effect";

import { acquireRelease, appFail, fiberSpawn, runMain, scope } from "../src/index.js";

const say = (text) => Effect.sync(() => process.stdout.write(`${text}\n`));

const resource = (release = say("released")) => acquireRelease(say("acquired"), () => release);

const waitForSignal = Effect.andThen(say("ready"), Effect.never);

const programs = {
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
};

const name = process.argv[2];
await runMain(
  programs[name],
  name === "old-effect" ? { capabilities: ["the Flick runtime requires effect 4.0.5, found 4.1.0"] } : {},
);
