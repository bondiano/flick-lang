// `Runner.Node` on Flick's own runtime: runs `main` under the root owner;
// SIGINT and SIGTERM interrupt it, and the exit code tells how it ended.
import { runMain } from "../entry.ts";

export const run = (main: (input: undefined) => unknown): Promise<void> => runMain(main);
