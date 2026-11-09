// `Runner.Node`: runs `main` on Node.js, with signals and the exit code.
import { asMain, runMain } from "../entry.ts";

export const run = (main: unknown): Promise<void> => runMain(asMain(main));
