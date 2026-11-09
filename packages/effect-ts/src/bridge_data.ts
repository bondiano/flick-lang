// Recursive data and callback conversions at foreign service boundaries.
import { Context, Exit as EExit, Option as EOption, Result as EResult } from "effect";
import { type Exit, succeeded, failed } from "@flick-lang/runtime/rt/cause";
import { currentCtx } from "@flick-lang/runtime/rt/fiber";
import { bridgeHooks } from "@flick-lang/runtime/rt/handle";
import { Failure } from "@flick-lang/runtime/rt/outcome";
import { err, none, ok, some } from "@flick-lang/runtime/core/data";
import { mapped } from "@flick-lang/runtime/rt/effects";
import { Failure as HostFailure } from "./cause.ts";
import { failureCause, flickExit, nativeExit } from "./stdlib/outcome.ts";
import { fromEffect, callEffect, causeFromEffect, causeToEffect } from "./bridge_runtime.ts";

const contextOf = (u: unknown): Context.Context<never> | undefined => Context.isContext(u) ? u : undefined;

/** A record of handlers the signature does not show: each function in it, as Effect's side calls one. */
export const handlersToEffect = (handlers: unknown): unknown => {
  if (typeof handlers === "function") {
    return (x: unknown) =>
      callEffect(() => {
        const out: unknown = Reflect.apply(handlers, undefined, [x]);
        return out;
      });
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
const serviceData = (value: unknown, toward: "host" | "flick", seen = new WeakMap<object, unknown>()): unknown => {
  if (typeof value !== "object" || value === null) return value;
  if (seen.has(value)) return seen.get(value);
  seen.set(value, value);
  const done = (mappedValue: unknown): unknown => {
    seen.set(value, mappedValue);
    return mappedValue;
  };
  if (toward === "flick") {
    if (value instanceof HostFailure) return done(failureFromEffect(value));
    if (EOption.isOption(value)) return done(EOption.isSome(value) ? some(serviceData(value.value, toward, seen)) : none);
    if (EResult.isResult(value))
      return done(EResult.isSuccess(value) ? ok(serviceData(value.success, toward, seen)) : err(serviceData(value.failure, toward, seen)));
  } else {
    if (value instanceof Failure) return done(failureToEffect(value));
    if (EOption.isOption(value) || EResult.isResult(value) || EExit.isExit(value)) return value;
    const tag: unknown = Reflect.get(value, "_tag");
    if (tag === "Some") return done(EOption.some(serviceData(Reflect.get(value, "value"), toward, seen)));
    if (tag === "None") return done(EOption.none());
    if (tag === "Success") return done(EResult.succeed(serviceData(Reflect.get(value, "success"), toward, seen)));
    if (tag === "Failure") return done(EResult.fail(serviceData(Reflect.get(value, "failure"), toward, seen)));
  }
  if (Array.isArray(value)) {
    const items = value.map((item) => serviceData(item, toward, seen));
    return done(items.every((item, i) => item === value[i]) ? value : items);
  }
  const prototype: unknown = Object.getPrototypeOf(value);
  if (prototype !== Object.prototype && prototype !== null) return value;
  const entries = Object.entries(value).map(([key, item]) => [key, serviceData(item, toward, seen)] as const);
  return done(entries.every(([key, item]) => item === Reflect.get(value, key)) ? value : Object.fromEntries(entries));
};
export const dataFromEffect = (value: unknown): unknown => serviceData(value, "flick");
export const dataToEffect = (value: unknown): unknown => serviceData(value, "host");
const hostOperation = (value: unknown): unknown =>
  typeof value === "function"
    ? (...args: ReadonlyArray<unknown>): unknown => hostOperation(Reflect.apply(value, undefined, args.map((arg) => serviceData(arg, "host"))))
    : value;

bridgeHooks.operation = (tag) => {
  const host = contextOf(currentCtx().host);
  const service: unknown = host?.mapUnsafe.get(String(Reflect.get(tag, "key")));
  if (service === undefined) return undefined;
  return new Proxy(Object(service), {
    get: (target, name) => hostOperation(Reflect.get(target, name)),
  });
};
bridgeHooks.ran = (value) => mapped(fromEffect(value), (result) => dataFromEffect(result));
