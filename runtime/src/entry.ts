// The entry runner: runs `main : Unit ->{none; AppError} Unit` under the root
// owner. SIGINT and SIGTERM interrupt it and close the root owner by the
// same protocol; the exit code is known only after cleanup.
import { Cause, Effect, Exit, Fiber } from "effect";

import { APP_ERROR, type AppError } from "./builtins.ts";
import { versionProblems } from "./capabilities.ts";
import { env, offSignal, onSignal, setExitCode, writeStderr } from "./host.ts";
import { reportedDefects, withOwner } from "./owner.ts";

const SIGNALS = { SIGINT: 130, SIGTERM: 143 } as const;

type Signal = keyof typeof SIGNALS;

const SIGNAL_NAMES: ReadonlyArray<Signal> = ["SIGINT", "SIGTERM"];

const write = writeStderr;

/** The text of a failure: an `Error`'s stack, else its message, else JSON. */
const describe = (u: unknown): string => {
  if (u instanceof Error) {
    // frames from the Effect runloop on say nothing about the program
    const frames = (u.stack ?? `${u.name}: ${u.message}`).split("\n");
    const end = frames.findIndex((line) => /Generator\.next|~effect\/(?:Effect|Utils)/.test(line));
    const own = frames.slice(0, end < 0 ? frames.length : end).join("\n");
    return u.cause === undefined ? own : `${own} {\n  [cause]: ${describe(u.cause)}\n}`;
  }
  if (typeof u === "string") return u;
  if (typeof u === "object" && u !== null) {
    const message: unknown = Reflect.get(u, "message");
    if (typeof message === "string") return message;
  }
  try {
    return JSON.stringify(u) ?? String(u);
  } catch {
    return String(u);
  }
};

/**
 * What went wrong in `cause`, one failure or defect after another. Effect's
 * `Cause.pretty` says the same with stack annotations, at the price of its
 * value formatter in every bundle.
 */
export const pretty = (cause: Cause.Cause<unknown>): string => {
  const failures = cause.reasons.flatMap((r) =>
    Cause.isFailReason(r) ? [describe(r.error)] : Cause.isDieReason(r) ? [describe(r.defect)] : [],
  );
  return failures.length > 0 ? failures.join("\n") : "InterruptError: All fibers interrupted without error";
};

const isAppError = (error: unknown): error is AppError =>
  typeof error === "object" && error !== null && Reflect.get(error, "_tag") === APP_ERROR;

/** The exit code of `exit`, reporting what went wrong on stderr. */
export const conclude = (
  exit: Exit.Exit<unknown, unknown>,
  signal: Signal | undefined,
  defects: ReadonlyArray<Cause.Cause<unknown>>,
): number => {
  for (const cause of defects) write(`error: a supervisor failed\n${pretty(cause)}\n`);
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
      : `${pretty(cause)}\n`,
  );
  return 1;
};

/** A `main` of the program: a function of its input, an effect or a plain value. */
export type Main<I> = (input: I) => unknown;

/** `main`, checked to be a function: a runner gets it from a module it cannot type. */
export const asMain = <I>(main: unknown): Main<I> => {
  if (typeof main !== "function") throw new TypeError("`main` is not a function");
  return (input) => {
    const result: unknown = Reflect.apply(main, undefined, [input]);
    return result;
  };
};

/** Runs `main` on `input` under the root owner; a pure `main` is done when it returns. */
export const start = <I>(main: Main<I>, input: I): Fiber.Fiber<unknown, unknown> =>
  Effect.runFork(
    withOwner(() =>
      Effect.suspend(() => {
        const result = main(input);
        // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiler checked `main` needs no service
        return Effect.isEffect(result) ? (result as Effect.Effect<unknown, unknown>) : Effect.succeed(result);
      }),
    ),
  );

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
  main: Main<undefined>,
  options: RunOptions = {},
): Promise<void> => {
  const problems = options.capabilities ?? versionProblems();
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
  const fiber = start(main, undefined);
  run.fiber = fiber;
  const exit = await new Promise<Exit.Exit<unknown, unknown>>((resolve) => fiber.addObserver(resolve));
  clearTimeout(run.timer);
  clearInterval(keepAlive);
  for (const [name, handler] of handlers) offSignal(name, handler);
  setExitCode(conclude(exit, run.signal, reportedDefects()));
};
