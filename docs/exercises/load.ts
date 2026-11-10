/** Build/CI-only TOML decoding. Only normalized exercise data reaches the browser. */
import { readFileSync } from "node:fs";
import path from "node:path";
import { parse } from "smol-toml";
import type { ExerciseCheck } from "../src/playground/exercise.ts";

export interface ExerciseMetadata {
  readonly check: ExerciseCheck;
  readonly title: string;
  readonly title_ru: string;
  readonly task: string;
  readonly task_ru: string;
  readonly hint: string;
  readonly hint_ru: string;
}
// Production prerendering relocates this module into a generated server chunk.
const root = path.join(process.cwd(), "exercises");
export const parseExerciseMetadata = (source: string, id: string): ExerciseMetadata => {
  const config = parse(source);
  const text = (key: string, allowEmpty = false): string => {
    const value = config[key];
    if (typeof value !== "string" || (!allowEmpty && value.length === 0)) throw new Error(`${id}: missing string ${key}`);
    return value;
  };
  const args = config.argument_types;
  if (!Array.isArray(args) || args.length === 0 || !args.every((arg): arg is string => typeof arg === "string" && /^(string|number|boolean)$/.test(arg))) throw new Error(`${id}: invalid argument_types`);
  const exported = text("export");
  const result = text("return_type");
  if (!/^[A-Za-z_]\w*$/.test(exported) || !/^(string|number|boolean|void)$/.test(result)) throw new Error(`${id}: invalid signature`);
  return {
    check: { export: exported, argument_types: args, return_type: result, output: text("output", true) },
    title: text("title"), title_ru: text("title_ru"), task: text("task"), task_ru: text("task_ru"), hint: text("hint"), hint_ru: text("hint_ru"),
  };
};

export interface Exercise extends ExerciseMetadata {
  readonly starter: string;
  readonly solution: string;
}
export const loadExercise = (id: string): Exercise => {
  if (!/^[a-z-]+\/\d+$/.test(id)) throw new Error(`Invalid exercise ID: ${id}`);
  const dir = path.join(root, id);
  return {
    ...parseExerciseMetadata(readFileSync(path.join(dir, "check.toml"), "utf8"), id),
    starter: readFileSync(path.join(dir, "starter.fl"), "utf8"),
    solution: readFileSync(path.join(dir, "solution.fl"), "utf8"),
  };
};
