/** Exercise facts and browser checks; Flick owns the lesson UI and its state. */
import { evaluateExercise } from "./exercise.ts";
import { lessonData } from "./dom.ts";
import { check, compileExercise, runBuilt } from "./compiler.ts";

export const checkLesson = async (source: string): Promise<string> => {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 10_000);
  try {
    const exercise = lessonData();
    if (!exercise) return "failed";
    let compilation: ReturnType<typeof compileExercise> | undefined;
    const compile = (): ReturnType<typeof compileExercise> => compilation ??= compileExercise(source, controller.signal);
    return await evaluateExercise(exercise.check, {
      check: () => check(source, controller.signal),
      compile,
      run: async () => {
        const execution = await runBuilt((await compile()).built, controller.signal);
        return { ...execution, code: Number(execution.detail) };
      },
    });
  } catch {
    return "failed";
  } finally {
    clearTimeout(timer);
  }
};
