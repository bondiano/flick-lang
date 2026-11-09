// What `flick test` does around the tests whatever runtime runs them: the
// options of the runner, the selection, the human and JSON reports, SIGINT and
// the end of a watcher's stdin, and the exit code.
import { type Named, type Test, flatten } from "./testing.ts";

export type FailureKind = "assertion" | "defect" | "interrupted" | "failure" | "cleanup";

/** The result of one test, as the reporters write it. */
export interface TestResult {
  readonly name: string;
  readonly status: "passed" | "failed";
  readonly kind?: FailureKind;
  readonly message?: string;
  readonly cleanup?: string;
}

/** Options of `flick test`. */
export interface TestOptions {
  readonly filter?: string;
  readonly jobs?: number;
  readonly seed?: number | undefined;
  readonly reporter?: string;
  readonly watched?: boolean;
  readonly color?: boolean;
  readonly write?: (text: string) => void;
  readonly progress?: boolean;
}

/** Options of `flick test`, from the runner's arguments. */
export const parseTestArgs = (argv: ReadonlyArray<string>): TestOptions => {
  const options: {
    filter: string;
    jobs: number;
    seed: number | undefined;
    reporter: string;
    watched: boolean;
    color: boolean;
    progress: boolean;
  } = {
    filter: "",
    jobs: 1,
    seed: undefined,
    reporter: "human",
    watched: false,
    color: false,
    progress: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    const value = argv[i + 1] ?? "";
    if (flag === "--filter") options.filter = value;
    else if (flag === "--jobs") options.jobs = Number(value);
    else if (flag === "--seed") options.seed = Number(value);
    else if (flag === "--reporter") options.reporter = value;
    else if (flag === "--color") options.color = value === "always";
    else if (flag === "--progress") options.progress = value === "live";
    else if (flag === "--watched") {
      options.watched = true;
      continue;
    } else continue;
    i++;
  }
  return options;
};

const humanResult = (
  result: TestResult,
  completed: number,
  total: number,
  elapsedMs: number,
  color: boolean,
): string => {
  const reset = color ? "\x1b[0m" : "";
  const dim = color ? "\x1b[2m" : "";
  const marker =
    result.status === "passed" ? `${color ? "\x1b[32m" : ""}✓${reset}` : `${color ? "\x1b[31m" : ""}✗${reset}`;
  const duration = Math.max(0, Math.floor(elapsedMs));
  let text = `  ${marker} ${dim}[${completed}/${total}]${reset} ${result.name}  ${dim}(${duration} ms)${reset}\n`;
  if (result.status === "failed") {
    text += `    ${`${result.kind ?? "failure"}: ${result.message ?? ""}`.replaceAll("\n", "\n    ")}\n`;
    if (result.cleanup && result.kind !== "cleanup") {
      text += `    ${`and cleanup failed: ${result.cleanup}`.replaceAll("\n", "\n    ")}\n`;
    }
  }
  return text;
};

const report = (
  results: ReadonlyArray<TestResult>,
  seed: number,
  reporter: string | undefined,
  write: (text: string) => void,
  color: boolean,
  elapsedMs: number,
): number => {
  const failed = results.filter((r) => r.status === "failed").length;
  const passed = results.length - failed;
  if (reporter === "json") {
    write(`${JSON.stringify({ seed, passed, failed, tests: results })}\n`);
    return failed;
  }
  const reset = color ? "\x1b[0m" : "";
  const dim = color ? "\x1b[2m" : "";
  const passedColor = color && passed > 0 ? "\x1b[32m" : dim;
  const failedColor = color && failed > 0 ? "\x1b[31m" : dim;
  write(`\n  Tests  ${passedColor}${passed}${reset} passed · ${failedColor}${failed}${reset} failed · ${results.length} total\n`);
  write(`  Time   ${dim}${Math.max(0, Math.floor(elapsedMs))} ms${reset}\n`);
  if (failed > 0) write(`seed: ${seed} (rerun with --seed ${seed})\n`);
  return failed;
};

/** The `tests` of each test module, by module name. */
export type Suites<Task, L> = ReadonlyArray<readonly [module: string, tests: ReadonlyArray<Test<Task, L>> | undefined]>;

/** What a run tells the reporter of each test, by its index among the selected. */
export interface Hooks {
  readonly started: (index: number, name: string) => void;
  readonly finished: (index: number, result: TestResult) => void;
}

/** A run of the selected tests a runtime started. */
export interface Started {
  /** Their results in order once every suite is released, or the text of what stopped the run. */
  readonly ended: Promise<ReadonlyArray<TestResult> | { readonly failure: string }>;
  readonly interrupt: () => void;
}

/** How a runtime runs `selected`, `jobs` at a time, calling `hooks` around each when they are given. */
export type Engine<Task, L> = (
  selected: ReadonlyArray<Named<Task, L>>,
  seed: number,
  jobs: number,
  hooks: Hooks | undefined,
) => Started;

const isEpipe = (error: unknown): boolean =>
  typeof error === "object" && error !== null && Reflect.get(error, "code") === "EPIPE";

/**
 * Runs the `tests` of each module by `engine`, at most `jobs` at a time, and
 * sets the exit code: 0 when all passed, 1 otherwise, 130 when SIGINT (or, for
 * a `watched` run, the end of stdin) stopped the run after every running
 * test's cleanup.
 */
export const runTestsWith = async <Task, L>(
  modules: Suites<Task, L>,
  options: TestOptions,
  engine: Engine<Task, L>,
): Promise<void> => {
  const write = options.write ?? ((text: string): void => void process.stdout.write(text));
  // a watcher that went away closes the pipe: nothing is left to report to
  process.stdout.on("error", (error) => {
    if (!isEpipe(error)) throw error;
  });
  const seed =
    options.seed !== undefined && Number.isInteger(options.seed) ? options.seed : Math.floor(Math.random() * 2 ** 31);
  const jobs = options.jobs !== undefined && Number.isInteger(options.jobs) && options.jobs > 0 ? options.jobs : 1;
  const selected = modules
    .flatMap(([module, tests]) => flatten(module, tests ?? []))
    .filter(({ name }) => name.includes(options.filter ?? ""));
  const human = options.reporter !== "json";
  const color = options.color ?? false;
  let completed = 0;
  const live = human && options.progress === true;
  const active = new Map<number, string>();
  const startedAt = new Map<number, number>();
  const frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
  let frame = 0;
  let timer: ReturnType<typeof setInterval> | undefined;
  const clear = (): void => {
    if (live) write("\r\x1b[2K");
  };
  const draw = (): void => {
    if (!live || active.size === 0) return;
    const name = active.values().next().value ?? "";
    const short = Array.from(name).slice(0, 22).join("");
    const elapsed = ((performance.now() - started) / 1000).toFixed(1);
    write(`\r\x1b[2K  ${frames[frame++ % frames.length]} Tests  ${completed}/${selected.length} complete · ${active.size} running · ${short}  ${elapsed}s`);
  };
  if (human) {
    const title = color ? "\x1b[1;36mflick test\x1b[0m" : "flick test";
    write(`  ${title}  ${selected.length} tests\n\n`);
  }
  const started = human ? performance.now() : 0;
  if (live) timer = setInterval(draw, 80);
  const hooks: Hooks | undefined = human
    ? {
        started: (index, name) => {
          startedAt.set(index, performance.now());
          active.set(index, name);
          draw();
        },
        finished: (index, result) => {
          active.delete(index);
          clear();
          const elapsed = performance.now() - (startedAt.get(index) ?? 0);
          write(humanResult(result, ++completed, selected.length, elapsed, color));
          draw();
        },
      }
    : undefined;
  const run = engine(selected, seed, jobs, hooks);
  let interrupted = false;
  const stop = (): void => {
    interrupted = true;
    run.interrupt();
  };
  process.once("SIGINT", stop);
  // a watcher stops the run by closing stdin, which works on every platform
  if (options.watched) process.stdin.once("end", stop).resume();
  const ended = await run.ended;
  process.off("SIGINT", stop);
  if (options.watched) process.stdin.off("end", stop).destroy();
  if (timer !== undefined) clearInterval(timer);
  clear();
  if ("failure" in ended) {
    write(interrupted ? "interrupted: every running test closed its scope\n" : `${ended.failure}\n`);
    process.exitCode = interrupted ? 130 : 1;
    return;
  }
  const failed = report(ended, seed, options.reporter, write, color, human ? performance.now() - started : 0);
  process.exitCode = failed > 0 ? 1 : 0;
};
