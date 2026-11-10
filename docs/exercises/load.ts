/** Build/CI-only TOML decoding. Only normalized exercise data reaches the browser. */
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { parse } from "smol-toml";
import type { ExerciseCheck } from "../src/playground/exercise.ts";

export interface Exercise {
  readonly id: string;
  readonly check: ExerciseCheck;
  readonly title: string;
  readonly title_ru: string;
  readonly task: string;
  readonly task_ru: string;
  readonly hint: string;
  readonly hint_ru: string;
  readonly starter: string;
  readonly solution: string;
}
const root = new URL("./", import.meta.url);
export const exerciseIds = (): Array<string> => readdirSync(root, { recursive: true, encoding: "utf8" }).map((name) => name.replaceAll("\\", "/")).filter((name) => name.endsWith("/check.toml")).map((name) => name.slice(0, -"/check.toml".length)).sort();
export const loadExercise = (id: string): Exercise => {
  if (!/^[a-z-]+\/\d+$/.test(id)) throw new Error(`Invalid exercise ID: ${id}`);
  const dir = new URL(`${id}/`, root);
  const config = parse(readFileSync(new URL("check.toml", dir), "utf8"));
  const text = (key: string): string => {
    const value = config[key];
    if (typeof value !== "string" || value.length === 0) throw new Error(`${id}: missing string ${key}`);
    return value;
  };
  const args = config.argument_types;
  if (!Array.isArray(args) || !args.every((arg): arg is string => typeof arg === "string" && /^(string|number|boolean)$/.test(arg))) throw new Error(`${id}: invalid argument_types`);
  const exported = text("export");
  const result = text("return_type");
  if (!/^\w+$/.test(exported) || !/^(string|number|boolean|void)$/.test(result)) throw new Error(`${id}: invalid signature`);
  return {
    id, check: { export: exported, argument_types: args, return_type: result, output: text("output") },
    title: text("title"), title_ru: text("title_ru"), task: text("task"), task_ru: text("task_ru"), hint: text("hint"), hint_ru: text("hint_ru"),
    starter: readFileSync(fileURLToPath(new URL("starter.fl", dir)), "utf8"),
    solution: readFileSync(fileURLToPath(new URL("solution.fl", dir)), "utf8"),
  };
};
