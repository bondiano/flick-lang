/** Exercise facts and browser checks; Flick owns the lesson UI and its state. */
import specification from "../../exercises/signatures/01/check.toml?raw";
import { check, compile, run } from "./compiler.ts";

const field = (name: string): string => {
  const value = new RegExp(`^${name} = "([^"]+)"$`, "m").exec(specification)?.[1];
  if (value === undefined) throw new Error(`Missing ${name} in lesson check.toml`);
  return value;
};
const exportName = field("export");
const returnType = field("return_type");
const output = field("output");

export const checkLesson = async (source: string): Promise<string> => {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 10_000);
  try {
    const problems = await check(source, controller.signal);
    if (problems.some((problem) => problem.severity === "error")) return "fix-errors";
    const generated = await compile(source, "debug", controller.signal);
    const signature = new RegExp(`\\b${exportName}:\\s*\\([^)]*: string\\)\\s*=>\\s*${returnType}\\b`);
    if (!signature.test(generated.dts)) return "type";
    const execution = await run(source, "debug", controller.signal);
    if (execution.status !== "exit" || execution.lines.map((line) => line.text).join("").trim() !== output) {
      return "output";
    }
    try { localStorage.setItem("flick.lesson.signatures.01.done", "1"); } catch { /* progress remains visible until reload */ }
    return "passed";
  } catch {
    return "failed";
  } finally {
    clearTimeout(timer);
  }
};
