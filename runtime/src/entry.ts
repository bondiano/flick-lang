// The entry runner: runs `main : Unit ->{none; AppError} Unit` under the root
// owner. SIGINT and SIGTERM interrupt it and close the root owner by the
// same protocol; the exit code is known only after cleanup.
import { Cause, Effect, Exit, Fiber } from "effect";

import { APP_ERROR, type AppError } from "./builtins.ts";
import { capabilities } from "./capabilities.ts";
import { env, offSignal, onSignal, setExitCode, writeStderr } from "./host.ts";
import { reportedDefects, withOwner } from "./owner.ts";

const SIGNALS = { SIGINT: 130, SIGTERM: 143 } as const;

type Signal = keyof typeof SIGNALS;

const SIGNAL_NAMES: ReadonlyArray<Signal> = ["SIGINT", "SIGTERM"];

const write = writeStderr;

const isAppError = (error: unknown): error is AppError =>
  typeof error === "object" && error !== null && Reflect.get(error, "_tag") === APP_ERROR;

/** The exit code of `exit`, reporting what went wrong on stderr. */
const conclude = (
  exit: Exit.Exit<unknown, unknown>,
  signal: Signal | undefined,
  defects: ReadonlyArray<Cause.Cause<unknown>>,
): number => {
  for (const cause of defects) write(`error: a supervisor failed\n${Cause.pretty(cause)}\n`);
  if (Exit.isSuccess(exit)) return defects.length > 0 ? 1 : 0;
  const { cause } = exit;
  if (Cause.hasInterruptsOnly(cause)) {
    if (signal && defects.length === 0) return SIGNALS[signal];
    write(signal ? "" : "error: main was interrupted\n");
    return 1;
  }
  const appError = cause.reasons.filter(Cause.isFailReason).find((r) => isAppError(r.error));
  write(
    appError && isAppError(appError.error) && !Cause.hasDies(cause)
      ? `error: ${appError.error._0.message}\n`
      : `${Cause.pretty(cause)}\n`,
  );
  return 1;
};

/** What a test of the runner overrides. */
export interface RunOptions {
  readonly capabilities?: ReadonlyArray<string>;
  readonly shutdownNotice?: number;
}

/**
 * Runs `main` and sets the exit code: 0 on success, 1 on a typed failure, a
 * defect or a failed cleanup, 130 or 143 when only a signal stopped it and
 * cleanup succeeded, 2 when the installed Effect is not the one the
 * runtime supports. A shutdown that takes long is reported on stderr.
 */
export const runMain = async (
  main: (unit: undefined) => Effect.Effect<unknown, unknown>,
  options: RunOptions = {},
): Promise<void> => {
  const problems = options.capabilities ?? capabilities();
  if (problems.length > 0) {
    for (const problem of problems) write(`error: ${problem}\n`);
    setExitCode(2);
    return;
  }
  const notice = options.shutdownNotice ?? Number(env("FLICK_SHUTDOWN_NOTICE_MS") ?? 2000);
  const run: { fiber?: Fiber.Fiber<unknown, unknown>; signal?: Signal; timer?: NodeJS.Timeout } = {};
  // installed before main starts: it may run up to a wait for the signal at once
  const handlers = SIGNAL_NAMES.map((name) => {
    const handler = (): void => {
      if (run.signal !== undefined || run.fiber === undefined) return;
      run.signal = name;
      run.timer = setTimeout(() => write("shutting down: waiting for cleanup to finish\n"), notice);
      run.timer.unref();
      Effect.runFork(Fiber.interrupt(run.fiber));
    };
    onSignal(name, handler);
    return [name, handler] as const;
  });
  // a fiber waiting on nothing but a signal must not let the process exit
  const keepAlive = setInterval(() => {}, 2 ** 30);
  const fiber = Effect.runFork(withOwner(() => main(undefined)));
  run.fiber = fiber;
  const exit = await new Promise<Exit.Exit<unknown, unknown>>((resolve) => fiber.addObserver(resolve));
  clearTimeout(run.timer);
  clearInterval(keepAlive);
  for (const [name, handler] of handlers) offSignal(name, handler);
  setExitCode(conclude(exit, run.signal, reportedDefects()));
};
