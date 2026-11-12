// A client of the Effect DevTools: the spans of a program, as they start and
// end, over a WebSocket in the DevTools' NDJSON protocol (Effect's
// `devtools/DevToolsSchema`, encoded as `Schema.toCodecJson` encodes it).
// Without DevTools listening the program runs as without them.
import { type Cause, type Exit } from "./cause.ts";
import { type Span, tracerHooks } from "./effect/Tracer.ts";
import { mainHooks } from "./entry.ts";

const NANOS_PER_MILLI = 1_000_000n;

/** A defect or a typed error as the DevTools' `Schema.Defect` encodes it. */
const defect = (value: unknown): unknown => {
  if (value instanceof Error) return { name: value.name, message: value.message, stack: value.stack };
  if (value === undefined) return null;
  if (typeof value === "bigint") return `${value}n`;
  try {
    const text = JSON.stringify(value);
    return text === undefined ? Object.prototype.toString.call(value) : JSON.parse(text);
  } catch {
    // a cycle or a BigInt inside: the tag of the value, not the value
    return Object.prototype.toString.call(value);
  }
};

const cause = (c: Cause): ReadonlyArray<unknown> =>
  c.reasons.map((r) =>
    r._tag === "Fail" ? { _tag: "Fail", error: defect(r.error) }
      : r._tag === "Die" ? { _tag: "Die", defect: defect(r.defect) }
      : { _tag: "Interrupt", fiberId: r.fiberId ?? null });

/** `span` as the DevTools see it: running, or ended with `exit`. */
const snapshot = (span: Span, exit?: Exit): unknown => {
  if (span.name === undefined) {
    return { _tag: "ExternalSpan", spanId: span.spanId, traceId: span.traceId, sampled: span.sampled };
  }
  const startTime = String(span.started * NANOS_PER_MILLI);
  return {
    _tag: "Span",
    spanId: span.spanId,
    traceId: span.traceId,
    name: span.name,
    sampled: span.sampled,
    attributes: span.attributes,
    status: exit === undefined || span.ended === undefined
      ? { _tag: "Started", startTime }
      : {
        _tag: "Ended",
        startTime,
        endTime: String(span.ended * NANOS_PER_MILLI),
        exit: exit.cause === undefined ? { _tag: "Success", value: null } : { _tag: "Failure", cause: cause(exit.cause) },
      },
    parent: span.parent === undefined ? { _tag: "None" } : { _tag: "Some", value: snapshot(span.parent) },
  };
};

/** Sends the spans of the program to the DevTools at `url` until `main` ends. */
export const devTools = (url: string): void => {
  const queue: Array<string> = [JSON.stringify({ _tag: "Ping" })];
  let socket: WebSocket | undefined;
  try {
    socket = new WebSocket(url);
  } catch {
    return;
  }
  const flush = (): void => {
    if (socket?.readyState !== WebSocket.OPEN) return;
    for (const line of queue.splice(0)) socket.send(`${line}\n`);
  };
  const opened = new Promise<boolean>((resolve) => {
    socket?.addEventListener("open", () => resolve(true), { once: true });
    socket?.addEventListener("error", () => resolve(false), { once: true });
  });
  socket.addEventListener("open", flush);
  const send = (message: unknown): void => {
    queue.push(JSON.stringify(message));
    flush();
  };
  const ping = setInterval(() => send({ _tag: "Ping" }), 3000);
  ping.unref();
  tracerHooks.started = (span) => send(snapshot(span));
  tracerHooks.ended = (span, exit) => send(snapshot(span, exit));
  mainHooks.push(async () => {
    clearInterval(ping);
    // as Effect's client: a second at most for the DevTools to answer
    const open = await Promise.race([opened, new Promise<boolean>((resolve) => setTimeout(() => resolve(false), 1000))]);
    if (!open || socket === undefined) {
      socket?.close();
      return;
    }
    flush();
    const closed = new Promise<void>((resolve) => socket?.addEventListener("close", () => resolve(), { once: true }));
    socket.close();
    await Promise.race([closed, new Promise<void>((resolve) => setTimeout(resolve, 1000))]);
  });
};
