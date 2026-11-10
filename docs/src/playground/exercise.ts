/** Shared rules over compiler results. Browser and CI supply their own compiler ports. */
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
const escape = (text: string): string => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

export const evaluateExercise = async (rules: ExerciseCheck, operations: ExerciseOperations): Promise<string> => {
  if ((await operations.check()).some((problem) => problem.severity === "error")) return "fix-errors";
  const generated = await operations.compile();
  const args = rules.argument_types.map((type) => `\\w+:\\s*${escape(type)}`).join(",\\s*");
  const signature = new RegExp(`^export declare const ${escape(rules.export)}:\\s*\\(\\s*${args}\\s*\\)\\s*=>\\s*${escape(rules.return_type)}\\s*;`, "m");
  if (!signature.test(generated.dts)) return "type";
  const execution = await operations.run();
  return execution.status === "exit" && execution.code === 0 && execution.lines.filter((line) => line.stream === "stdout").map((line) => line.text).join("").trim() === rules.output ? "passed" : "output";
};
