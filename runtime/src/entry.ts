// The entry runner: runs `main : Unit ->{none; AppError} Unit` under the root
// owner. SIGINT and SIGTERM interrupt it and close the root owner by the same
// protocol; the exit code is known only after cleanup.
import { tag } from "./tags.ts";
import { type Cause, type Exit, hasDies, hasInterruptsOnly, isDie, isFail } from "./cause.ts";
import { currentCtx, fork } from "./fiber.ts";
import { env, offSignal, onSignal, setExitCode, writeStderr } from "./host.ts";
import { reportedDefects, traceExit, withOwner } from "./owner.ts";
import { traceEvent, tracing } from "./trace.ts";

const SIGNALS = { SIGINT: 130, SIGTERM: 143 } as const;

type Signal = keyof typeof SIGNALS;

const SIGNAL_NAMES: ReadonlyArray<Signal> = ["SIGINT", "SIGTERM"];

const APP_ERROR = /*#__PURE__*/ tag("Prelude.AppError.AppError");

/** The text of a failure: an `Error`'s stack, else its message, else JSON. */
const describe = (u: unknown): string => {
  if (u instanceof Error) {
    // frames from the driver on say nothing about the program
    const frames = (u.stack ?? `${u.name}: ${u.message}`).split("\n");
    const end = frames.findIndex((line) => /Generator\.(?:next|throw)|\/rt\/fiber\.[jt]s/.test(line));
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

/** What went wrong in `cause`, one failure or defect after another. */
export const pretty = (cause: Cause): string => {
  const failures = cause.reasons.flatMap((r) =>
    isFail(r) ? [describe(r.error)] : isDie(r) ? [describe(r.defect)] : [],
  );
  return failures.length > 0 ? failures.join("\n") : "InterruptError: All fibers interrupted without error";
};

const appMessage = (cause: Cause): string | undefined => {
  if (hasDies(cause)) return undefined;
  for (const r of cause.reasons) {
    if (isFail(r) && typeof r.error === "object" && r.error !== null && Reflect.get(r.error, "_tag") === APP_ERROR) {
      const fields: unknown = Reflect.get(r.error, "_0");
      const message: unknown = Reflect.get(Object(fields), "message");
      if (typeof message === "string") return message;
    }
  }
  return undefined;
};

/** The exit code of `exit`, reporting what went wrong on stderr. */
export const conclude = (exit: Exit, signal: Signal | undefined, defects: ReadonlyArray<Cause>): number => {
  for (const cause of defects) writeStderr(`error: a supervisor failed\n${pretty(cause)}\n`);
  const { cause } = exit;
  if (cause === undefined) return defects.length > 0 ? 1 : 0;
  if (hasInterruptsOnly(cause)) {
    if (signal && defects.length === 0) return SIGNALS[signal];
    writeStderr(signal ? "" : "error: main was interrupted\n");
    return 1;
  }
  const message = appMessage(cause);
  writeStderr(message === undefined ? `${pretty(cause)}\n` : `error: ${message}\n`);
  return 1;
};

/**
 * Runs `main` and sets the exit code: 0 on success, 1 on a typed failure, a
 * defect or a failed cleanup, 130 or 143 when only a signal stopped it and
 * cleanup succeeded. A shutdown that takes long is reported on stderr.
 */
export const runMain = async (main: (input: undefined) => unknown): Promise<void> => {
  const notice = Number(env("FLICK_SHUTDOWN_NOTICE_MS") ?? 2000);
  let signalled: Signal | undefined;
  let timer: NodeJS.Timeout | undefined;
  const fiber = fork(() => withOwner(() => main(undefined), "main"), currentCtx());
  const handlers = SIGNAL_NAMES.map((name) => {
    const handler = (): void => {
      if (signalled !== undefined) return;
      signalled = name;
      timer = setTimeout(() => writeStderr("shutting down: waiting for cleanup to finish\n"), notice);
      timer.unref();
      fiber.interrupt(-1);
    };
    onSignal(name, handler);
    return [name, handler] as const;
  });
  // a fiber waiting on nothing but a signal must not let the process exit
  const keepAlive = setInterval(() => {}, 2 ** 30);
  const exit = await new Promise<Exit>((resolve) => {
    fiber.observe(resolve);
  });
  clearTimeout(timer);
  clearInterval(keepAlive);
  for (const [name, handler] of handlers) offSignal(name, handler);
  if (tracing) traceEvent({ e: "exit", x: traceExit(exit) });
  setExitCode(conclude(exit, signalled, reportedDefects()));
};
