// Late-bound interpreter operations let the value adapters depend on the
// kernel without a module cycle. The public bridge installs them on import.
import type { Effect } from "effect";
import type { Cause as HostCause } from "effect/Cause";
import type { Cause } from "@flick-lang/runtime/rt/cause";

interface BridgeKernel {
  fromEffect(effect: unknown): unknown;
  callEffect(call: () => unknown): Effect.Effect<unknown, unknown>;
  causeFromEffect(cause: HostCause<unknown>): Cause;
  causeToEffect(cause: Cause): HostCause<unknown>;
}
const absent = (): never => { throw new Error("the Flick Effect bridge has not been initialized"); };
export const bridgeKernel: BridgeKernel = {
  fromEffect: absent, callEffect: absent, causeFromEffect: absent, causeToEffect: absent,
};
export const fromEffect = (effect: unknown): unknown => bridgeKernel.fromEffect(effect);
export const callEffect = (call: () => unknown): Effect.Effect<unknown, unknown> => bridgeKernel.callEffect(call);
export const causeFromEffect = (cause: HostCause<unknown>): Cause => bridgeKernel.causeFromEffect(cause);
export const causeToEffect = (cause: Cause): HostCause<unknown> => bridgeKernel.causeToEffect(cause);
