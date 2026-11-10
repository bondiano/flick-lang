import assert from "node:assert/strict";
import { test } from "node:test";
import { evaluateExercise } from "../src/playground/exercise.ts";

const contract = { export: "length_of", argument_types: ["string"], return_type: "number", output: "5" };
const operations = {
  check: async () => [],
  compile: async () => ({ dts: "export declare const length_of: (text: string) => number;" }),
  run: async () => ({ status: "exit", code: 0, lines: [{ stream: "stdout", text: "5\n" }] }),
};
test("rules control signature and output, including compilable wrong answers", async () => {
  assert.equal(await evaluateExercise(contract, operations), "passed");
  assert.equal(await evaluateExercise(contract, { ...operations, run: async () => ({ status: "exit", code: 1, lines: [{ stream: "stdout", text: "5" }] }) }), "output");
  assert.equal(await evaluateExercise(contract, { ...operations, run: async () => ({ status: "exit", code: 0, lines: [{ stream: "stderr", text: "5" }] }) }), "output");
  assert.equal(await evaluateExercise({ ...contract, output: "6" }, operations), "output");
  assert.equal(await evaluateExercise({ ...contract, argument_types: ["number"] }, operations), "type");
  assert.equal(await evaluateExercise({ ...contract, export: "length.of" }, operations), "type");
  assert.equal(await evaluateExercise(contract, { ...operations, compile: async () => ({ dts: "export declare const other: { length_of: (text: string) => number; };" }) }), "type");
  assert.equal(await evaluateExercise(contract, { ...operations, check: async () => [{ severity: "error" }] }), "fix-errors");
  assert.equal(await evaluateExercise(contract, { ...operations, run: async () => ({ status: "error", code: 1, lines: [{ stream: "stdout", text: "5" }] }) }), "output");
});

test("multiple arguments use Flick's curried declaration", async () => {
  assert.equal(await evaluateExercise({ ...contract, argument_types: ["string", "number"], return_type: "string" }, {
    ...operations,
    compile: async () => ({ dts: "export declare const length_of: (arg: string) => (arg: number) => string;" }),
  }), "passed");
});

test("a Unit result uses the emitted undefined type", async () => {
  assert.equal(await evaluateExercise({ ...contract, return_type: "void" }, {
    ...operations,
    compile: async () => ({ dts: "export declare const length_of: (arg: string) => undefined;" }),
  }), "passed");
});
