// Layers share memoization and finalizer ownership across both interpreters.
import { Context, Effect, Exit as EExit, Layer as ELayer, Scope as EScope } from "effect";
import { type Exit, succeeded, failed } from "@flick-lang/runtime/rt/cause";
import { type Run, lookup } from "@flick-lang/runtime/rt/fiber";
import { HOST, Layer, MEMO as MEMO_KEY, MemoMap, layerHooks, memoizable } from "@flick-lang/runtime/rt/layer";
import { Owner, addTo, close, inside } from "@flick-lang/runtime/rt/owner";
import { fromEffect, callEffect, causeFromEffect, causeToEffect } from "./bridge_runtime.ts";
import { servicesOut } from "./bridge_host.ts";

const contextOf = (u: unknown): Context.Context<never> | undefined => Context.isContext(u) ? u : undefined;

/** One memo map of Effect per memo map of the kernel: an Effect layer shared inside Effect is built once. */
const memos = /*#__PURE__*/ new WeakMap<MemoMap, ELayer.MemoMap>();
const memoOf = (memo: MemoMap): ELayer.MemoMap => {
  const found = memos.get(memo);
  if (found !== undefined) return found;
  const made = memo.parent === undefined ? ELayer.makeMemoMapUnsafe() : ELayer.forkMemoMapUnsafe(memoOf(memo.parent));
  memos.set(memo, made);
  return made;
};

const adopted = /*#__PURE__*/ new WeakMap<object, Layer>();

/** An Effect layer as the kernel's: built into a scope of Effect that the owner closes, its services under `HOST`. */
const adopt = (layer: unknown): Layer => {
  if (layer instanceof Layer) return layer;
  if (!ELayer.isLayer(layer)) throw new TypeError("not a layer of this runtime or of Effect");
  const found = adopted.get(layer);
  if (found !== undefined) return found;
  const made = memoizable(function* (memo: MemoMap, into: Owner): Run {
    const scope = EScope.makeUnsafe();
    yield addTo(
      into,
      (exit: Exit) => fromEffect(EScope.close(scope, exit.cause === undefined ? EExit.void : EExit.failCause(causeToEffect(exit.cause)))),
      true,
    );
    const context: unknown = yield inside(into, () =>
      fromEffect(ELayer.buildWithMemoMap(layer, memoOf(memo), scope)),
    );
    return [[HOST, context]];
  });
  adopted.set(layer, made);
  return made;
};

/** A foreign Effect layer entering Flick's runtime. */
export const layerFromEffect = (layer: unknown): Layer => adopt(layer);

/** A kernel layer as Effect's: built by a fiber of the kernel into an owner the Effect scope closes. */
export const layerToEffect = (layer: unknown): unknown => {
  if (!(layer instanceof Layer)) return layer;
  return ELayer.effectContext(
    Effect.flatMap(Effect.scope, (scope) =>
      Effect.map(
        callEffect(function* (): Run {
          const owner = new Owner();
          EScope.addFinalizerExit(scope, (exit) =>
            // a close of an owner fails only with defects and interruptions
            Effect.orDie(callEffect(() => close(owner, exit._tag === "Success" ? succeeded(undefined) : failed(causeFromEffect(exit.cause))))),
          ).pipe(Effect.runSync);
          // the memo map of the `provide` around it: a layer shared with the kernel's side is one build
          const outer = lookup(MEMO_KEY);
          const memo = outer instanceof MemoMap ? outer : new MemoMap(undefined);
          return yield inside(owner, () => layer.build(memo, owner));
        }),
        servicesOut,
      ),
    ),
  );
};

export const installLayerBridge = (): void => {
layerHooks.adopt = adopt;
layerHooks.services = (value) => Context.isContext(value) ? [[HOST, value]] : undefined;
layerHooks.context = servicesOut;
layerHooks.merge = (a, b) => {
  const first = contextOf(a);
  const second = contextOf(b);
  return first === undefined || second === undefined ? (first ?? b) : Context.merge(first, second);
};

};
