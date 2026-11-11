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
  /** Shown one at a time, in order; none for an exercise to solve alone. */
  readonly hints: ReadonlyArray<string>;
  readonly hints_ru: ReadonlyArray<string>;
  /** The one error code the starter has; CI checks it, the browser only asks to fix errors. */
  readonly starter_error: string;
  /** A question to answer as well, by the index of its right choice. */
  readonly question?: Question;
}
export interface Question {
  readonly text: string;
  readonly text_ru: string;
  readonly choices: ReadonlyArray<string>;
  readonly choices_ru: ReadonlyArray<string>;
  readonly answer: number;
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
  const runs = config.runs ?? false;
  if (typeof runs !== "boolean") throw new Error(`${id}: invalid runs`);
  const texts = (key: string): ReadonlyArray<string> => {
    const value = config[key];
    if (!Array.isArray(value) || !value.every((item): item is string => typeof item === "string" && item.length > 0)) throw new Error(`${id}: invalid ${key}`);
    return value;
  };
  // one `hint` is a list of one; `hints = []` leaves the exercise without any
  const [hints, hintsRu] = config.hints === undefined ? [[text("hint")], [text("hint_ru")]] : [texts("hints"), texts("hints_ru")];
  if (hints.length !== hintsRu.length) throw new Error(`${id}: hints and hints_ru differ in length`);
  let question: Question | undefined;
  if (config.question !== undefined) {
    const choices = texts("choices");
    const choicesRu = texts("choices_ru");
    const answer = config.answer;
    if (choices.length < 2 || choices.length !== choicesRu.length) throw new Error(`${id}: invalid choices`);
    if (typeof answer !== "number" || !Number.isInteger(answer) || answer < 0 || answer >= choices.length) throw new Error(`${id}: invalid answer`);
    question = { text: text("question"), text_ru: text("question_ru"), choices, choices_ru: choicesRu, answer };
  }
  const starterError = config.starter_error ?? "FLICK-T003";
  if (typeof starterError !== "string" || !/^FLICK-[A-Z]\d{3}$/.test(starterError)) throw new Error(`${id}: invalid starter_error`);
  return {
    check: { export: exported, argument_types: args, return_type: result, output: text("output", true), ...(runs ? { runs } : {}) },
    title: text("title"), title_ru: text("title_ru"), task: text("task"), task_ru: text("task_ru"),
    hints, hints_ru: hintsRu,
    starter_error: starterError,
    ...(question ? { question } : {}),
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
