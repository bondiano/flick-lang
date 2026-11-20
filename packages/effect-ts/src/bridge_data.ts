// Recursive data and callback conversions at foreign service boundaries.
import { Context, Effect, Exit as EExit, Option as EOption, Result as EResult } from "effect";
import { type Exit, succeeded, failed } from "@flick-lang/runtime/rt/cause";
import { currentCtx } from "@flick-lang/runtime/rt/fiber";
import { bridgeHooks } from "@flick-lang/runtime/rt/handle";
import { Failure } from "@flick-lang/runtime/rt/outcome";
import { ChunkBranch } from "@flick-lang/runtime/core/chunk";
import { err, none, ok, some } from "@flick-lang/runtime/core/data";
import { chunkToList } from "@flick-lang/runtime/rt/chunk";
import { mapped } from "@flick-lang/runtime/rt/effects";
import { Failure as HostFailure } from "./cause.ts";
import { failureCause, flickExit, nativeExit } from "./stdlib/outcome.ts";
import { fromEffect, callEffect, causeFromEffect, causeToEffect } from "./bridge_runtime.ts";

const contextOf = (u: unknown): Context.Context<never> | undefined => Context.isContext(u) ? u : undefined;

/** A record of handlers the signature does not show: each function in it, as Effect's side calls one. */
export const handlersToEffect = (handlers: unknown): unknown => {
  if (typeof handlers === "function") {
    return (x: unknown) =>
      Effect.map(callEffect(() => Reflect.apply(handlers, undefined, [dataFromEffect(x)])), dataToEffect);
  }
  if (typeof handlers !== "object" || handlers === null) return handlers;
  return Object.fromEntries(Object.entries(handlers).map(([k, v]) => [k, handlersToEffect(v)]));
};

export const optionToEffect = (value: { readonly _tag: string; readonly value?: unknown }, f: (a: unknown) => unknown): unknown =>
  value._tag === "Some" ? EOption.some(f(value.value)) : EOption.none();
export const optionFromEffect = (value: { readonly _tag: string; readonly value?: unknown }, f: (a: unknown) => unknown): unknown =>
  value._tag === "Some" ? some(f(value.value)) : none;

// `Exit`, `Failure` at the boundary: the kernel's and Effect's are different classes.
export const exitToEffect = (exit: Exit): unknown =>
  flickExit(exit.cause === undefined ? EExit.succeed(exit.value) : EExit.failCause(causeToEffect(exit.cause)));
export const exitFromEffect = (exit: never): Exit => {
  const native = nativeExit<unknown, unknown>(exit);
  return native._tag === "Success" ? succeeded(native.value) : failed(causeFromEffect(native.cause));
};
export const failureToEffect = (failure: Failure): unknown => new HostFailure(causeToEffect(failure.cause));
export const failureFromEffect = (failure: HostFailure): Failure => new Failure(causeFromEffect(failureCause(failure)));
export const resultToEffect = (
  value: { readonly _tag: string; readonly success?: unknown; readonly failure?: unknown },
  f: (a: unknown) => unknown,
  g: (e: unknown) => unknown,
): unknown => (value._tag === "Success" ? EResult.succeed(f(value.success)) : EResult.fail(g(value.failure)));
export const resultFromEffect = (
  value: { readonly _tag: string; readonly success?: unknown; readonly failure?: unknown },
  f: (a: unknown) => unknown,
  g: (e: unknown) => unknown,
): unknown => (value._tag === "Success" ? ok(f(value.success)) : err(g(value.failure)));

/** Service operations have no signature at this runtime boundary; convert their data recursively. */
const serviceData = (value: unknown, toward: "host" | "flick"): unknown => {
  const seen = new WeakMap<object, unknown>();
  const work: Array<() => void> = [];
  let result: unknown;
  const visit = (input: unknown, put: (mappedValue: unknown) => void): void => {
    if (typeof input !== "object" || input === null) { put(input); return; }
    if (seen.has(input)) { put(seen.get(input)); return; }
    // Preserve the generic bridge's existing active-cycle and completed-alias
    // behavior. Unlike defensive copying, unchanged graphs keep their identity.
    seen.set(input, input);
    const finish = (mappedValue: unknown): void => { seen.set(input, mappedValue); put(mappedValue); };
    const variant = (child: unknown, wrap: (item: unknown) => unknown): void => {
      work.push(() => { visit(child, (item) => { work.push(() => { finish(wrap(item)); }); }); });
    };
    if (toward === "flick") {
      if (input instanceof HostFailure) { finish(failureFromEffect(input)); return; }
      if (EOption.isOption(input)) {
        if (EOption.isSome(input)) variant(input.value, some);
        else finish(none);
        return;
      }
      if (EResult.isResult(input)) {
        if (EResult.isSuccess(input)) variant(input.success, ok);
        else variant(input.failure, err);
        return;
      }
    } else {
      if (input instanceof Failure) { finish(failureToEffect(input)); return; }
      if (EOption.isOption(input) || EResult.isResult(input) || EExit.isExit(input)) { put(input); return; }
      const tag: unknown = Reflect.get(input, "_tag");
      if (tag === "Some") { variant(Reflect.get(input, "value"), EOption.some); return; }
      if (tag === "None") { finish(EOption.none()); return; }
      if (tag === "Success") { variant(Reflect.get(input, "success"), EResult.succeed); return; }
      if (tag === "Failure") { variant(Reflect.get(input, "failure"), EResult.fail); return; }
    }
    const array = input instanceof ChunkBranch ? chunkToList(input) : Array.isArray(input) ? input : undefined;
    if (array !== undefined) {
      const length = array.length;
      const items: unknown[] = [];
      items.length = length;
      let index = 0;
      const next = (): void => {
        while (index < length) {
          const key = index++;
          if (!(key in array)) continue;
          const child: unknown = array[key];
          work.push(next);
          visit(child, (item) => { Object.defineProperty(items, key, { value: item, writable: true, enumerable: true, configurable: true }); });
          return;
        }
        finish(items.every((item, i) => item === array[i]) ? array : items);
      };
      work.push(next);
      return;
    }
    const prototype: unknown = Object.getPrototypeOf(input);
    if (prototype !== Object.prototype && prototype !== null) { put(input); return; }
    const entries: Array<[string, unknown]> = Object.entries(input);
    const converted: Array<readonly [string, unknown]> = [];
    let index = 0;
    const next = (): void => {
      const entry = entries[index++];
      if (entry === undefined) {
        finish(converted.every(([key, item]) => item === Reflect.get(input, key)) ? input : Object.fromEntries(converted));
        return;
      }
      const [key, child] = entry;
      work.push(next);
      visit(child, (item) => { converted.push([key, item]); });
    };
    work.push(next);
  };
  visit(value, (mappedValue) => { result = mappedValue; });
  while (work.length > 0) work.pop()?.();
  return result;
};
export const dataFromEffect = (value: unknown): unknown => serviceData(value, "flick");
export const dataToEffect = (value: unknown): unknown => serviceData(value, "host");
const hostOperation = (value: unknown): unknown =>
  typeof value === "function"
    ? (...args: ReadonlyArray<unknown>): unknown => hostOperation(Reflect.apply(value, undefined, args.map((arg) => serviceData(arg, "host"))))
    : value;

export const installDataBridge = (): void => {
bridgeHooks.operation = (tag) => {
  const host = contextOf(currentCtx().host);
  const service: unknown = host?.mapUnsafe.get(String(Reflect.get(tag, "key")));
  if (service === undefined) return undefined;
  return new Proxy(Object(service), {
    get: (target, name) => hostOperation(Reflect.get(target, name)),
  });
};
bridgeHooks.ran = (value) => mapped(fromEffect(value), (result) => dataFromEffect(result));
};
