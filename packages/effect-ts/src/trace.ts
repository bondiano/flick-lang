// The runtime trace of docs/formal/runtime.md: with FLICK_TRACE set to
// a file, each step of the owner contract λRT names is written there as one
// JSON object per line, in the order the steps happened. The events and
// their fields are those of the Go runtime (runtime-go/flickrt/trace.go),
// which lists them. A disabled trace costs one check of `tracing`.
//
// shortcut: finalizers Effect adds itself (of layers, channels, servers) are not seen here, so neither runtime writes them; trace them when the monitor needs them
import { traceId } from "@flick-lang/runtime/rt/trace";

export { traceEvent, traceId, traceLayer, tracing } from "@flick-lang/runtime/rt/trace";

const children = /*#__PURE__*/ new WeakMap<object, number>();

/** The id of a child `fiberSpawn` registered, if it is one. */
export const spawnedChild = (fiber: object): number | undefined => children.get(fiber);

/** The id of a child, given at its spawn. */
export const traceChild = (fiber: object): number => {
  let id = children.get(fiber);
  if (id === undefined) {
    id = traceId();
    children.set(fiber, id);
  }
  return id;
};
