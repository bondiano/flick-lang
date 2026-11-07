// A service built by Effect's Layer, with a real acquire and release, and one
// that needs a service Flick implements.
import { Effect, service } from "@flick-lang/runtime";
import * as Layer from "@flick-lang/runtime/effect/Layer";
const log = (s) => Effect.sync(() => process.stdout.write(`${s}\n`));
let builds = 0;
export const dbLayer = (tag) =>
  Layer.effect(tag, Effect.acquireRelease(
    Effect.gen(function* () {
      yield* log(`effect layer: acquire Db, build ${++builds}`);
      yield* Effect.sleep(5);
      return { query: (q) => Effect.as(Effect.sleep(1), `db answered ${q}`) };
    }),
    () => Effect.gen(function* () {
      yield* log("effect layer: release Db starts");
      yield* Effect.sleep(20);
      yield* log("effect layer: release Db ends");
    }),
  ));
export const usesFlick = (tag, greet) =>
  Layer.effect(tag, Effect.gen(function* () {
    const g = yield* Effect.service(service(greet.key));
    const hello = yield* g.greet("from effect's layer");
    yield* log(`effect layer read a Flick service: ${hello}`);
    return { query: (q) => Effect.succeed(`${hello} ${q}`) };
  }));
export const httpGet = (url) => fetch(url).then(async (r) => `${r.status} ${await r.text()}`, (e) => `failed ${e}`);
export const httpPost = (url, body) => fetch(url, { method: "POST", body, headers: { "content-type": "application/json" } }).then(async (r) => `${r.status} ${await r.text()}`, (e) => `failed ${e}`);
