import { spawn } from "node:child_process";
import { once } from "node:events";
import { createServer } from "node:net";
import { setTimeout } from "node:timers/promises";

const available = async (url) => {
  const socket = createServer();
  const target = new URL(url);
  await new Promise((resolve, reject) => {
    socket.once("error", (error) => reject(new Error(`startup port is occupied or unavailable: ${url}`, { cause: error })));
    socket.listen({ host: target.hostname, port: Number(target.port || 80), exclusive: true }, resolve);
  });
  await new Promise((resolve, reject) => socket.close((error) => error ? reject(error) : resolve()));
};

export const stopServer = async (child) => {
  if (child.pid === undefined || child.exitCode !== null || child.signalCode !== null) return;
  const ended = once(child, "exit");
  child.kill();
  const force = globalThis.setTimeout(() => child.kill("SIGKILL"), 5000);
  force.unref();
  try { await ended; } finally { clearTimeout(force); }
};

/** Start an owned server on a free port; readiness must precede its exit. */
export const startServer = async (command, { url, env = {}, cwd, pollMs = 10, timeoutMs = 30_000 }) => {
  await available(url);
  const started = performance.now();
  const child = spawn(command[0], command.slice(1), {
    cwd,
    env: { ...process.env, ...env, PORT: new URL(url).port },
    stdio: "ignore",
  });
  const failed = new AbortController();
  const exited = (code, signal) => failed.abort(new Error(`startup process exited before readiness: ${code ?? signal}`));
  const broken = (error) => failed.abort(error);
  child.once("exit", exited);
  child.once("error", broken);
  const signal = AbortSignal.any([failed.signal, AbortSignal.timeout(timeoutMs)]);
  try {
    for (;;) {
      signal.throwIfAborted();
      try {
        const response = await fetch(url, { signal });
        await response.text();
        signal.throwIfAborted();
        if (response.status === 200) {
          return { child, milliseconds: performance.now() - started };
        }
      } catch {
        signal.throwIfAborted();
      }
      await setTimeout(pollMs, undefined, { signal });
    }
  } catch (error) {
    // Preserve the failure that caused cleanup; SIGTERM is only its consequence.
    const reason = signal.reason ?? error;
    await stopServer(child);
    throw reason;
  } finally {
    child.off("exit", exited);
    child.off("error", broken);
  }
};

/** Each sample owns a fresh process and reaps it before starting the next. */
export const startupTimes = async (command, options, runs) => {
  if (!Number.isSafeInteger(runs) || runs <= 0) throw new Error("startup needs a positive run count");
  const samples = [];
  for (let i = 0; i < runs; i++) {
    const { child, milliseconds } = await startServer(command, options);
    try {
      samples.push(milliseconds);
    } finally {
      await stopServer(child);
    }
  }
  return samples;
};
