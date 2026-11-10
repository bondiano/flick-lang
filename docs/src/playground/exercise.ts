/** Shared rules over compiler results. Browser and CI supply their own compiler ports. */
import { escape } from "@flick-lang/runtime/rt/effect/RegExp";
export interface ExerciseCheck {
  readonly export: string;
  readonly argument_types: ReadonlyArray<string>;
  readonly return_type: string;
  readonly output: string;
}
export interface ExerciseOperations {
  check(): Promise<ReadonlyArray<{ readonly severity: string }>>;
  compile(): Promise<{ readonly dts: string }>;
  run(): Promise<{ readonly status: string; readonly code: number; readonly lines: ReadonlyArray<{ readonly stream: string; readonly text: string }> }>;
}

export const evaluateExercise = async (rules: ExerciseCheck, operations: ExerciseOperations): Promise<string> => {
  if ((await operations.check()).some((problem) => problem.severity === "error")) return "fix-errors";
  const generated = await operations.compile();
  const args = rules.argument_types.map((type) => `\\(\\s*[$\\w]+:\\s*${escape(type)}\\s*\\)\\s*=>\\s*`).join("");
  const result = rules.return_type === "void" ? "undefined" : rules.return_type;
  // Flick identifiers cannot contain $, which the emitter appends to reserved JS names.
  const signature = new RegExp(`^export declare const ${escape(rules.export)}\\$?:\\s*${args}${escape(result)}\\s*;`, "m");
  if (!signature.test(generated.dts)) return "type";
  const execution = await operations.run();
  return execution.status === "exit" && execution.code === 0 && execution.lines.filter((line) => line.stream === "stdout").map((line) => line.text).join("").trim() === rules.output ? "passed" : "output";
};
