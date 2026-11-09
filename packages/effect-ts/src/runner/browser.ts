// `Runner.Browser`: runs `main` in a browser or a worker, where there are no
// signals and no exit code; what went wrong goes to the console. A page that
// mounts a program itself passes its `main` and the input it takes.
import { Effect, type Exit, Fiber } from "effect";

import { versionProblems } from "../capabilities.ts";
import { asMain, conclude, start } from "../entry.ts";
import { writeStderr } from "../host.ts";
import { reportedDefects } from "../owner.ts";
import { hotCarryTo, hotStates } from "../hot.ts";

/** The running program, which a module replacement in development stops. */
let current: Fiber.Fiber<unknown, unknown> | undefined;

export const run = async (main: unknown, input?: unknown): Promise<void> => {
  const problems = versionProblems();
  for (const problem of problems) writeStderr(`error: ${problem}\n`);
  if (problems.length > 0) return;
  const fiber = start(asMain(main), input);
  current = fiber;
  const exit = await new Promise<Exit.Exit<unknown, unknown>>((resolve) => fiber.addObserver(resolve));
  // a replaced program was stopped on purpose
  if (current === fiber) conclude(exit, undefined, reportedDefects());
};

/**
 * Development: stops the running program, which unmounts its roots and
 * ends their watchers, and gives the values of its labelled states for the
 * next one. A page's entry calls it from `import.meta.hot.dispose`.
 */
export const hotDispose = (): Map<string, unknown> => {
  const states = hotStates();
  const fiber = current;
  current = undefined;
  if (fiber !== undefined) Effect.runFork(Fiber.interrupt(fiber));
  return states;
};

/** Development: the next program's roots start from `states`, what `hotDispose` gave. */
export const hotCarry = (states: ReadonlyMap<string, unknown> | undefined): void => {
  if (states !== undefined) hotCarryTo(states);
};
