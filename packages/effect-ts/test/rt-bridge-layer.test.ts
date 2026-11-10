import assert from "node:assert/strict";
import { test } from "node:test";
import { Context, Effect, Layer } from "effect";
import { currentCtx, fork } from "@flick-lang/runtime/rt/fiber";
import { layerRun, layerSucceed } from "@flick-lang/runtime/rt/layer";
import { service } from "@flick-lang/runtime/rt";
import { fromEffect, layerFromEffect, layerToEffect } from "../src/bridge.ts";

test("sibling Effect scopes build separate native layers and close each acquisition", async () => {
  let acquired = 0;
  let released = 0;
  const key = Context.Service<number>("Test.RequestLayer");
  const native = layerFromEffect(Layer.effect(key, Effect.acquireRelease(
    Effect.sync(() => ++acquired),
    () => Effect.sync(() => { released++; }),
  )));
  const host = layerToEffect(native);
  assert.ok(Layer.isLayer(host));
  const request = Effect.provide(Effect.flatMap(Effect.service(key), (id) => Effect.as(Effect.sleep(5), id)), host);
  const values = await new Promise<unknown>((resolve, reject) => {
    fork(() => layerRun(layerSucceed(service("Test.Outer"), {}), () => fromEffect(
      Effect.all([request, request], { concurrency: "unbounded" }),
    )), currentCtx()).observe((exit) => {
      if (exit.cause === undefined) resolve(exit.value);
      else reject(new Error(JSON.stringify(exit.cause)));
    });
  });
  assert.deepEqual(values, [1, 2]);
  assert.equal(acquired, 2);
  assert.equal(released, 2);
});

test("an Effect scope reuses a dependency already acquired by its native parent", async () => {
  let acquired = 0;
  let released = 0;
  const key = Context.Service<number>("Test.ParentLayer");
  const native = layerFromEffect(Layer.effect(key, Effect.acquireRelease(
    Effect.sync(() => ++acquired),
    () => Effect.sync(() => { released++; }),
  )));
  const host = layerToEffect(native);
  assert.ok(Layer.isLayer(host));
  const value = await new Promise<unknown>((resolve, reject) => {
    fork(() => layerRun(native, () => fromEffect(Effect.provide(Effect.service(key), host))), currentCtx()).observe((exit) => {
      if (exit.cause === undefined) resolve(exit.value);
      else reject(new Error(JSON.stringify(exit.cause)));
    });
  });
  assert.equal(value, 1);
  assert.equal(acquired, 1);
  assert.equal(released, 1);
});
