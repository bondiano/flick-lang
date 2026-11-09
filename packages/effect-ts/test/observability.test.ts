import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";

test("observability converts objects without process and keeps stderr consoles Node-only", () => {
  const result = spawnSync(
    process.execPath,
    [
      "--input-type=module",
      "--eval",
      `
        import assert from "node:assert/strict";
        import { registerHooks } from "node:module";
        const nodeProcess = process;
        const hooks = registerHooks({
          resolve(specifier, context, next) {
            if (specifier === "node:util" || specifier === "node:console" || specifier.startsWith("@effect/platform-node")) {
              throw new Error("loaded Node-only module: " + specifier);
            }
            return next(specifier, context);
          },
        });
        Reflect.deleteProperty(globalThis, "process");
        // Dynamic imports exercise loading only after the process-less host and rejection hook are installed.
        const { Cause, Effect } = await import("effect");
        const Console = await import("@flick-lang/effect-ts/effect/Console");
        const Logger = await import("@flick-lang/effect-ts/effect/Logger");
        const Tracer = await import("@flick-lang/effect-ts/effect/Tracer");
        const ErrorReporter = await import("@flick-lang/effect-ts/effect/ErrorReporter");
        assert.equal(typeof process, "undefined");
        Effect.runSync(Console.log("portable log"));
        const object = { answer: 42 };
        const expected = '{"answer":42}';
        const logger = Logger.flickMake((options) => options);
        const logOptions = {
          message: ["message", object],
          cause: Cause.empty,
          logLevel: "Info",
          date: new Date(0),
        };
        assert.deepEqual(logger.log(logOptions).message, ["message", expected]);
        const span = Tracer.flickExternalSpan({ spanId: "span", traceId: "trace", sampled: true });
        assert.deepEqual(Tracer.flickSpanLinks([{ span, attributes: { object, text: "unchanged" } }])[0].attributes, [
          ["object", expected], ["text", "unchanged"],
        ]);
        const reports = [];
        const error = Object.assign(new Error("broken"), {
          [ErrorReporter.attributes]: { object, text: "unchanged" },
        });
        ErrorReporter.flickMake((report) => reports.push(report)).report({
          cause: Cause.fail(error), timestamp: 1n,
        });
        assert.equal(reports[0].message, "broken");
        assert.deepEqual(reports[0].attributes, [["object", expected], ["text", "unchanged"]]);
        assert.throws(() => Console.flickToStderr(), /node:console is available only on Node.js/);
        hooks.deregister();
        globalThis.process = nodeProcess;
        const { Console: NodeConsole } = await import("node:console");
        const stderrConsole = Console.flickToStderr();
        assert.ok(stderrConsole instanceof NodeConsole);
        stderrConsole.log("stderr log");
        assert.deepEqual(logger.log(logOptions).message, ["message", "{ answer: 42 }"]);
        stderrConsole.error("stderr error");
      `,
    ],
    { cwd: new URL("..", import.meta.url), encoding: "utf8" },
  );
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, "portable log\n");
  assert.equal(result.stderr, "stderr log\nstderr error\n");
});
