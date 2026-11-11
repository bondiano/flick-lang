/** Shared rules over compiler results. Browser and CI supply their own compiler ports. */
import { resolve } from "./loader.ts";
import { escape } from "@flick-lang/runtime/rt/effect/RegExp";
export interface ExerciseCheck {
  readonly export: string;
  readonly argument_types: ReadonlyArray<string>;
  readonly return_type: string;
  readonly output: string;
  /** The export runs (`->{}`): its result comes in an Effect that needs and fails with nothing. */
  readonly runs?: boolean;
}
export interface ExerciseDeclarations {
  readonly dts: string;
  readonly declarations?: Readonly<Record<string, string>>;
  readonly declarationEntry?: string;
}
export interface ExerciseOperations {
  check(): Promise<ReadonlyArray<{ readonly severity: string }>>;
  compile(): Promise<ExerciseDeclarations>;
  run(): Promise<{ readonly status: string; readonly code: number; readonly lines: ReadonlyArray<{ readonly stream: string; readonly text: string }> }>;
}

export const evaluateExercise = async (rules: ExerciseCheck, operations: ExerciseOperations): Promise<string> => {
  if ((await operations.check()).some((problem) => problem.severity === "error")) return "fix-errors";
  const generated = await operations.compile();
  const args = rules.argument_types.map((type) => `\\(\\s*[$\\w]+:\\s*${escape(type)}\\s*\\)\\s*=>\\s*`).join("");
  const value = escape(rules.return_type === "void" ? "undefined" : rules.return_type);
  // the Effect build declares `$Effect.Effect<…>`, Flick's own runtime `$Effect<…>`
  const result = rules.runs ? `\\$Effect(?:\\.Effect)?<${value},\\s*never,\\s*never>` : value;
  const entry = generated.declarationEntry ?? "Main.d.ts";
  const declarations = { ...generated.declarations, [entry]: generated.dts };
  const seen = new Set<string>();
  const matches = (file: string, name: string): boolean => {
    const key = `${file}\0${name}`;
    if (seen.has(key)) return false;
    seen.add(key);
    const text = declarations[file] ?? "";
    const signature = new RegExp(`^export declare const ${escape(name)}:\\s*${args}${result}\\s*;`, "m");
    if (signature.test(text)) return true;
    for (const reexport of text.matchAll(/^export\s*\{([^}]+)\}\s*from\s*"([^"]+)";/gm)) {
      const specifier = reexport[2] ?? "";
      if (!specifier.startsWith("./") && !specifier.startsWith("../")) continue;
      for (const exported of (reexport[1] ?? "").split(",")) {
        const [original, alias] = exported.trim().split(/\s+as\s+/);
        if (original && (alias ?? original) === name && matches(resolve(file, specifier).replace(/\.js$/, ".d.ts"), original)) return true;
      }
    }
    return false;
  };
  // The emitter appends $ to reserved JS names; Flick identifiers cannot contain $.
  if (!matches(entry, rules.export) && !matches(entry, `${rules.export}$`)) return "type";
  const execution = await operations.run();
  return execution.status === "exit" && execution.code === 0 && execution.lines.filter((line) => line.stream === "stdout").map((line) => line.text).join("").trim() === rules.output ? "passed" : "output";
};
