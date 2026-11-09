// The runtime trace of docs/formal/runtime.md: with FLICK_TRACE set to a file,
// each step of the owner contract is one JSON object per line. The events and
// their fields are those of runtime-go/flickrt/trace.go, which lists them. A
// disabled trace costs one check of `tracing`.
import { nodeFs, takeEnv } from "./host.ts";

// taken: a process or worker this one starts writes no trace over this one's
const path = /*#__PURE__*/ takeEnv("FLICK_TRACE");

/** Whether events are written. */
export const tracing: boolean = path !== undefined && path !== "";

let file: number | undefined;
let ids = 0;

/** A fresh id of an owner, a child, a layer or a foreign call. */
export const traceId = (): number => ++ids;

/** Writes one event. */
export const traceEvent = (event: Readonly<Record<string, string | number>>): void => {
  if (path === undefined) return;
  const fs = nodeFs();
  file ??= fs.openSync(path, "w");
  fs.writeSync(file, `${JSON.stringify(event)}\n`);
};

const layers = /*#__PURE__*/ new WeakMap<object, number>();

/** The id of a layer, given when it first builds. */
export const traceLayer = (layer: object): number => {
  let id = layers.get(layer);
  if (id === undefined) {
    id = traceId();
    layers.set(layer, id);
  }
  return id;
};
