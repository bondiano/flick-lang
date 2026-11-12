// `Console` on Flick's own runtime: the host's console, or the one
// `with_console` gives a task, as Effect's `Console` service is. A group or a
// timer of a scope ends when the scope closes.
import type { Option } from "../../core/data.ts";
import { type Run, Under, currentCtx, extend, lookup } from "../fiber.ts";
import { acquireRelease, bracket } from "../owner.ts";

const CONSOLE = /*#__PURE__*/ Object.freeze({ kind: "rt/console" });

const isConsole = (value: unknown): value is Console =>
  typeof value === "object" && value !== null && typeof Reflect.get(value, "log") === "function";

/** The console the running task writes to. */
const console_ = (): Console => {
  const given = lookup(CONSOLE);
  return isConsole(given) ? given : globalThis.console;
};

export const flickCurrent = (): Console => console_();

export const flickWithConsole = (console: Console) => (task: () => unknown): Under =>
  new Under(extend(currentCtx(), CONSOLE, console), task);

/** `Console.to_stderr`: a Node.js console on stderr only. */
export const flickToStderr = (): Console => {
  const nodeConsole = typeof process === "undefined" ? undefined : process.getBuiltinModule("node:console");
  if (nodeConsole === undefined) throw new Error("Console.to_stderr is available only on Node.js");
  return new nodeConsole.Console(process.stderr, process.stderr);
};

export const log = (message: string): void => console_().log(message);
export const info = (message: string): void => console_().info(message);
export const debug = (message: string): void => console_().debug(message);
export const warn = (message: string): void => console_().warn(message);
export const error = (message: string): void => console_().error(message);
export const trace = (message: string): void => console_().trace(message);
export const assert = (condition: boolean, message: string): void => console_().assert(condition, message);
export const flickClear = (): void => console_().clear();
export const count = (label: string): void => console_().count(label);
export const countReset = (label: string): void => console_().countReset(label);
export const dir = (item: unknown): void => console_().dir(item);
export const dirxml = (item: unknown): void => console_().dirxml(item);

/** `Console.table`: the columns first, `None` for all of them. */
export const flickTable = (properties: Option<ReadonlyArray<string>>, data: unknown): void =>
  properties._tag === "Some" ? console_().table(data, [...properties.value]) : console_().table(data);

interface GroupOptions {
  readonly label: Option<string>;
  readonly collapsed: boolean;
}

const open = (options: GroupOptions): Console => {
  const console = console_();
  const label = options.label._tag === "Some" ? [options.label.value] : [];
  if (options.collapsed) console.groupCollapsed(...label);
  else console.group(...label);
  return console;
};

const close = (console: unknown): void => {
  if (isConsole(console)) console.groupEnd();
};

/** `Console.group`: until the innermost scope closes. */
export const flickGroup = (options: GroupOptions): Run => acquireRelease(() => open(options), close);

/** `Console.with_group`: around `task`, however it ends. */
export const flickWithGroup = (options: GroupOptions) => (task: () => unknown): Run =>
  bracket(() => open(options), task, close);

const started = (label: string) => (): Console => {
  const console = console_();
  console.time(label);
  return console;
};

const ended = (label: string) => (console: unknown): void => {
  if (isConsole(console)) console.timeEnd(label);
};

/** `Console.time`: until the innermost scope closes. */
export const time = (label: string): Run => acquireRelease(started(label), ended(label));

export const timeLog = (label: string, message: string): void => console_().timeLog(label, message);

/** `Console.with_time`: around `task`, however it ends. */
export const withTime = (label: string) => (task: () => unknown): Run => bracket(started(label), task, ended(label));
