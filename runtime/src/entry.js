// The entry runner: runs `main : Unit ->{none; AppError} Unit` under the root
// owner. SIGINT and SIGTERM interrupt it and close the root owner by the
// same protocol; the exit code is known only after cleanup.
import { Cause, Effect, Exit, Fiber } from "effect";

import { APP_ERROR } from "./builtins.js";
import { capabilities } from "./capabilities.js";
import { reportedDefects, withOwner } from "./owner.js";

const SIGNALS = { SIGINT: 130, SIGTERM: 143 };

const write = (text) => process.stderr.write(text);

/** The exit code of `exit`, reporting what went wrong on stderr. */
const conclude = (exit, signal, defects) => {
  for (const cause of defects) write(`error: a supervisor failed\n${Cause.pretty(cause)}\n`);
  if (Exit.isSuccess(exit)) return defects.length > 0 ? 1 : 0;
  const { cause } = exit;
  if (Cause.hasInterruptsOnly(cause)) {
    if (signal && defects.length === 0) return SIGNALS[signal];
    write(signal ? "" : "error: main was interrupted\n");
    return 1;
  }
  const appError = cause.reasons.find(
    (r) => Cause.isFailReason(r) && r.error?._tag === APP_ERROR,
  );
  write(
    appError && !Cause.hasDies(cause)
      ? `error: ${appError.error._0.message}\n`
      : `${Cause.pretty(cause)}\n`,
  );
  return 1;
};

/**
 * Runs `main` and sets the exit code: 0 on success, 1 on a typed failure, a
 * defect or a failed cleanup, 130 or 143 when only a signal stopped it and
 * cleanup succeeded, 2 when the installed Effect is not the one the
 * runtime supports. A shutdown that takes long is reported on stderr.
 */
export const runMain = async (main, options = {}) => {
  const problems = options.capabilities ?? capabilities();
  if (problems.length > 0) {
    for (const problem of problems) write(`error: ${problem}\n`);
    process.exitCode = 2;
    return;
  }
  const notice = options.shutdownNotice ?? Number(process.env.FLICK_SHUTDOWN_NOTICE_MS ?? 2000);
  let fiber;
  let signal;
  let timer;
  // installed before main starts: it may run up to a wait for the signal at once
  const handlers = Object.keys(SIGNALS).map((name) => {
    const handler = () => {
      if (signal !== undefined) return;
      signal = name;
      timer = setTimeout(() => write("shutting down: waiting for cleanup to finish\n"), notice);
      timer.unref();
      Effect.runFork(Fiber.interrupt(fiber));
    };
    process.on(name, handler);
    return [name, handler];
  });
  // a fiber waiting on nothing but a signal must not let the process exit
  const keepAlive = setInterval(() => {}, 2 ** 30);
  fiber = Effect.runFork(withOwner(() => main()));
  const exit = await new Promise((resolve) => fiber.addObserver(resolve));
  clearTimeout(timer);
  clearInterval(keepAlive);
  for (const [name, handler] of handlers) process.off(name, handler);
  process.exitCode = conclude(exit, signal, reportedDefects());
};
