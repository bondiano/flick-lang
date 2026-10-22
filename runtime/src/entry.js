// The entry runner: runs `main : Unit ->{none; AppError} Unit` and sets the
// exit code from how it ended.
import { Cause, Effect, Exit } from "effect";

import { APP_ERROR } from "./builtins.js";

/**
 * Exit code 0 on success; 1 on a typed `AppError` (its message goes to
 * stderr) or any other failure (the cause is printed).
 */
export const runMain = async (main) => {
  const exit = await Effect.runPromiseExit(main());
  if (Exit.isSuccess(exit)) {
    process.exitCode = 0;
    return;
  }
  const appError = exit.cause.reasons.find(
    (reason) => Cause.isFailReason(reason) && reason.error?._tag === APP_ERROR,
  );
  process.stderr.write(
    appError ? `error: ${appError.error._0.message}\n` : `${Cause.pretty(exit.cause)}\n`,
  );
  process.exitCode = 1;
};
