import assert from "node:assert/strict";
import { test } from "node:test";

import { click, container, domRenderer, gated, logging, stop, tick } from "./ui-helpers.ts";

import { Effect, Logger, References, Tracer } from "effect";

import { label } from "../src/atoms.ts";
import { WriteService, atomLayer, make, nativeRegistry } from "../src/effect/reactivity/Atom.ts";
import { sync, syncOperation } from "../src/index.ts";
import { type Handler, type Ui, actionExhaust, actionLatest, actionWith, uiButton, uiRunWith, uiText } from "../src/ui/react/index.ts";

interface Recorded {
  readonly name: string;
  readonly attributes: ReadonlyMap<string, unknown>;
  readonly events: Array<string>;
}

/** A tracer that keeps each span it makes, with the names of its events. */
const recording = () => {
  const spans: Array<Recorded> = [];
  const tracer = Tracer.make({
    span(options) {
      const span = Tracer.nativeTracer.span(options);
      const events: Array<string> = [];
      const event = span.event.bind(span);
      span.event = (name, startTime, attributes) => {
        events.push(name);
        event(name, startTime, attributes);
      };
      spans.push({ name: span.name, attributes: span.attributes, events });
      return span;
    },
  });
  return { spans, tracer };
};

const button = (handler: Handler) => uiButton({ disabled: false, on_click: handler }, [uiText("go")]);

const mountTraced = (root: Element, ui: Ui, tracer: Tracer.Tracer) =>
  Effect.runFork(Effect.withTracer(uiRunWith(domRenderer(root), logging([]), ui, undefined), tracer));

test("a run of an action is a span named by its label, with its policy and event", async () => {
  const { spans, tracer } = recording();
  const log: Array<string> = [];
  const { run, release } = gated(log);
  const go = label("App.go", actionExhaust(() => run(1)));
  const root = container();
  const fiber = mountTraced(root, button(go), tracer);
  click(root);
  click(root);
  await tick();
  release();
  await tick();
  assert.equal(spans.length, 1);
  assert.equal(spans[0]?.name, "App.go");
  assert.equal(spans[0]?.attributes.get("flick.action.policy"), "exhaust");
  assert.equal(spans[0]?.attributes.get("flick.event"), "click");
  // the second click came while the first task ran
  assert.deepEqual(spans[0]?.events, ["rejected"]);
  await stop(fiber);
});

test("an interrupted run and a replaced event are events of the span", async () => {
  const { spans, tracer } = recording();
  const log: Array<string> = [];
  const { run } = gated(log);
  let n = 0;
  const go = label("App.go", actionLatest(() => run(++n)));
  const root = container();
  const fiber = mountTraced(root, button(go), tracer);
  click(root);
  await tick(1);
  click(root);
  click(root);
  await tick(40);
  assert.deepEqual(
    spans.map((span) => span.name),
    ["App.go", "App.go"],
  );
  assert.deepEqual(spans[0]?.events, ["rejected", "interrupted"]);
  await stop(fiber);
});

test("an action given an argument keeps the label of its span", async () => {
  const { spans, tracer } = recording();
  const go = label("App.remove", actionExhaust(() => Effect.void));
  const root = container();
  const fiber = mountTraced(root, button(actionWith(go, 1)), tracer);
  click(root);
  await tick();
  assert.equal(spans[0]?.name, "App.remove");
  await stop(fiber);
});

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `Atom.set` as a Flick task calls it
const set = syncOperation(WriteService, "set", 3) as (place: string) => (atom: unknown) => (value: unknown) => unknown;

/** The debug lines of a root whose action `App.go` writes `App.count`, logged from `level` up. */
const writes = async (level: "Debug" | "Info"): Promise<Array<string>> => {
  const lines: Array<string> = [];
  const logger = Logger.make(({ message }) => {
    lines.push((Array.isArray(message) ? message : [message]).map(String).join(" "));
  });
  const count = label("App.count", make(0));
  const go = label("App.go", actionExhaust(() => sync(() => set("App:7")(count)(1))));
  const root = container();
  const program = Effect.gen(function* () {
    const registry = nativeRegistry(yield* Effect.service(WriteService));
    return yield* uiRunWith(domRenderer(root), logging([]), button(go), registry);
  }).pipe(
    Effect.provide(atomLayer),
    Effect.provideService(References.MinimumLogLevel, level),
    Effect.provideService(Logger.CurrentLoggers, new Set([logger])),
  );
  const fiber = Effect.runFork(program);
  await tick();
  click(root);
  await tick();
  await stop(fiber);
  return lines;
};

test("a write in an action logs its atom, action, event, span, fiber and place at the debug level", async () => {
  const lines = await writes("Debug");
  assert.equal(lines.length, 1);
  assert.match(lines[0] ?? "", /^atom App\.count ← action App\.go ← event click ← span \w+ \(fiber #\d+\) at App:7$/u);
});

test("a write logs nothing above the debug level", async () => {
  assert.deepEqual(await writes("Info"), []);
});
