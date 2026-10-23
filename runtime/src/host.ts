// The host a program runs on: Node.js, or a browser or worker, where the
// standard streams are the console and there are no arguments, environment,
// signals or files. Node's modules are reached through
// `process.getBuiltinModule`, so a bundle for the browser imports none.
type SignalName = "SIGINT" | "SIGTERM";

interface Process {
  readonly stdout: { write(text: string): unknown };
  readonly stderr: { write(text: string): unknown };
  readonly argv: ReadonlyArray<string>;
  readonly env: Readonly<Record<string, string | undefined>>;
  exitCode?: number | string | null | undefined;
  on(signal: SignalName, handler: () => void): unknown;
  off(signal: SignalName, handler: () => void): unknown;
  readonly getBuiltinModule?: NodeJS.Process["getBuiltinModule"];
}

const host = (): Process | undefined => (globalThis as { process?: Process }).process;

export const writeStdout = (text: string): void => {
  const p = host();
  if (p === undefined) console.log(text.replace(/\n$/, ""));
  else p.stdout.write(text);
};

export const writeStderr = (text: string): void => {
  const p = host();
  if (p === undefined) console.error(text.replace(/\n$/, ""));
  else p.stderr.write(text);
};

export const args = (): Array<string> => host()?.argv.slice(2) ?? [];

export const env = (name: string): string | undefined => host()?.env[name];

export const setExitCode = (code: number): void => {
  const p = host();
  if (p !== undefined) p.exitCode = code;
};

export const onSignal = (signal: SignalName, handler: () => void): void => void host()?.on(signal, handler);

export const offSignal = (signal: SignalName, handler: () => void): void => void host()?.off(signal, handler);

const builtin = (): NodeJS.Process["getBuiltinModule"] | undefined => host()?.getBuiltinModule;

/** `node:util`, or `undefined` on another host. */
export const nodeUtil = () => builtin()?.("node:util");

const required = <T>(module: T | undefined, id: string): T => {
  if (module === undefined) throw new Error(`${id} is available only on Node.js`);
  return module;
};

export const nodeFs = () => required(builtin()?.("node:fs"), "node:fs");
export const nodeFsPromises = () => required(builtin()?.("node:fs/promises"), "node:fs/promises");
export const nodeUrl = () => required(builtin()?.("node:url"), "node:url");
