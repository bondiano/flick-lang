// The host a program runs on: Node.js, or a browser or worker, where the
// standard streams are the console and there are no signals or files.
type SignalName = "SIGINT" | "SIGTERM";

interface Process {
  readonly stdout: { write(text: string): unknown };
  readonly stderr: { write(text: string): unknown };
  readonly env: Readonly<Record<string, string | undefined>>;
  exitCode?: number | string | null | undefined;
  on(signal: SignalName, handler: () => void): unknown;
  off(signal: SignalName, handler: () => void): unknown;
  readonly getBuiltinModule?: NodeJS.Process["getBuiltinModule"];
}

const host = (): Process | undefined => (globalThis as { process?: Process }).process;

export const writeStderr = (text: string): void => {
  const p = host();
  if (p === undefined) console.error(text.replace(/\n$/, ""));
  else p.stderr.write(text);
};

export const env = (name: string): string | undefined => host()?.env[name];

/** The variable `name`, removed so that the processes and workers this one starts do not inherit it. */
export const takeEnv = (name: string): string | undefined => {
  const p = host();
  const value = p?.env[name];
  if (value !== undefined) Reflect.deleteProperty(p?.env ?? {}, name);
  return value;
};

export const setExitCode = (code: number): void => {
  const p = host();
  if (p !== undefined) p.exitCode = code;
};

export const onSignal = (signal: SignalName, handler: () => void): void => void host()?.on(signal, handler);

export const offSignal = (signal: SignalName, handler: () => void): void => void host()?.off(signal, handler);

/** `node:fs`; the trace is written only on Node.js. */
export const nodeFs = () => {
  const fs = host()?.getBuiltinModule?.("node:fs");
  if (fs === undefined) throw new Error("node:fs is available only on Node.js");
  return fs;
};
