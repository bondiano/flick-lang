import { run } from "../playground/compiler.ts";

// Compiler/runtime interop; source choice, run state and errors are owned by Hero.fl.
export const runDemo = (source: string, signal: AbortSignal) => run(source, "debug", signal);
