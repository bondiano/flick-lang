// `Runner.Browser` on Flick's own runtime: runs `main` in a browser or a
// worker, where there are no signals and no exit code; what went wrong goes
// to the console. A page that mounts a program itself passes its `main` and
// the input it takes.
import { type Exit } from "../cause.ts";
import { conclude } from "../entry.ts";
import { type Fiber, currentCtx, fork } from "../fiber.ts";
import { hotCarryTo, hotStates } from "../hot.ts";
import { reportedDefects, withOwner } from "../owner.ts";

/** The running program, which a module replacement in development stops. */
let current: Fiber | undefined;

export const run = async (main: (input: unknown) => unknown, input?: unknown): Promise<void> => {
  const fiber = fork(() => withOwner(() => main(input), "main"), currentCtx());
  current = fiber;
  const exit = await new Promise<Exit>((resolve) => {
    fiber.observe(resolve);
  });
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
  fiber?.interrupt(-1);
  return states;
};

/** Development: the next program's roots start from `states`, what `hotDispose` gave. */
export const hotCarry = (states: ReadonlyMap<string, unknown> | undefined): void => {
  if (states !== undefined) hotCarryTo(states);
};
