// `Runner.Browser`: runs `main` in a browser or a worker, where there are no
// signals and no exit code; what went wrong goes to the console. A page that
// mounts a program itself passes its `main` and the input it takes.
import { versionProblems } from "../capabilities.ts";
import { asMain, conclude, start } from "../entry.ts";
import { writeStderr } from "../host.ts";
import { reportedDefects } from "../owner.ts";

export const run = async (main: unknown, input?: unknown): Promise<void> => {
  const problems = versionProblems();
  for (const problem of problems) writeStderr(`error: ${problem}\n`);
  if (problems.length > 0) return;
  const fiber = start(asMain(main), input);
  conclude(await new Promise((resolve) => fiber.addObserver(resolve)), undefined, reportedDefects());
};
