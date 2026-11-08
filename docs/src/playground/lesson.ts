/** Exercise facts and browser checks; Flick owns the lesson UI and its state. */
import { lessonCheck } from "./lesson-check.ts";
import { check, compile, run } from "./compiler.ts";

export const checkLesson = async (source: string): Promise<string> => {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 10_000);
  try {
    const problems = await check(source, controller.signal);
    if (problems.some((problem) => problem.severity === "error")) return "fix-errors";
    const generated = await compile(source, "debug", controller.signal);
    const signature = new RegExp(`\\b${lessonCheck.exportName}:\\s*\\([^)]*: string\\)\\s*=>\\s*${lessonCheck.returnType}\\b`);
    if (!signature.test(generated.dts)) return "type";
    const execution = await run(source, "debug", controller.signal);
    if (execution.status !== "exit" || execution.lines.map((line) => line.text).join("").trim() !== lessonCheck.output) {
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
