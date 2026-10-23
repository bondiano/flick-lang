import { load, run } from "../playground/compiler.ts";

// Compiler/runtime interop; source choice, run state and errors are owned by Hero.fl.
export const runDemo = async (source: string, signal: AbortSignal) => {
  await load();
  return run(source, "debug", signal);
};
