/* oxlint-disable typescript/no-unsafe-type-assertion -- a console that records what it is told is a Console here */
// `Console` on Flick's own runtime: the console `with_console` gives a task,
// and groups and timers that end however the task does.
import assert from "node:assert/strict";
import { test } from "node:test";

import { fail, raise } from "../src/rt/cause.ts";
import { flickWithConsole, flickWithGroup, log, withTime } from "../src/rt/effect/Console.ts";
import { none } from "../src/core/data.ts";
import { root } from "./rt-support.ts";

const recording = () => {
  const told: Array<string> = [];
  const say = (name: string) => (...args: ReadonlyArray<unknown>) => told.push([name, ...args].join(" "));
  const console = { log: say("log"), group: say("group"), groupEnd: say("groupEnd"), time: say("time"), timeEnd: say("timeEnd") };
  return { told, console: console as unknown as Console };
};

test("a task writes to the console it is given, and a group or a timer ends when it fails", async () => {
  const { told, console } = recording();
  const exit = await root(() =>
    flickWithConsole(console)(() =>
      flickWithGroup({ label: none, collapsed: false })(() =>
        withTime("t")(() => {
          log("inside");
          return raise(fail("boom"));
        }),
      ),
    ),
  );
  assert.equal(exit.cause?.reasons[0]?._tag, "Fail");
  assert.deepEqual(told, ["group", "time t", "log inside", "timeEnd t", "groupEnd"]);
});
