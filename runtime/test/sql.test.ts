import assert from "node:assert/strict";
import { test } from "node:test";

import { BigDecimal, Effect, Exit, Fiber, Option } from "effect";

import {
  type PgClient,
  type PgPool,
  type Template,
  databaseOf,
  sqlBind,
  sqlIdentifier,
  sqlRawIdent,
  sqlRawParam,
  sqlRawQuery,
  sqlRawText,
  sqlTable,
  sqlTemplate,
  sqlText,
} from "../src/sql/index.ts";
import { codecString } from "../src/schema/codecs.ts";
import { errorsOf } from "./support.ts";

interface Call {
  readonly text: string;
  readonly values: ReadonlyArray<unknown>;
}

/** A pool of one connection whose queries answer `reply` or never. */
const fakePool = (reply: (call: Call) => Promise<Awaited<ReturnType<PgClient["query"]>>>) => {
  const log: Array<string> = [];
  const calls: Array<Call> = [];
  let ended: (() => void) | undefined;
  const client: PgClient = {
    query: (config) => {
      calls.push({ text: config.text, values: config.values });
      return reply(config);
    },
    release: (destroy) => {
      log.push(destroy === undefined ? "released" : "destroying");
      if (destroy !== undefined) setTimeout(() => {
        log.push("closed");
        ended?.();
      }, 20);
    },
    once: (_event, listener) => {
      ended = listener;
      return client;
    },
  };
  const pool: PgPool = {
    connect: () => {
      log.push("connect");
      return Promise.resolve(client);
    },
    end: () => Promise.resolve(),
  };
  return { pool, log, calls };
};

const users = sqlTable({
  name: "users",
  columns: [
    { name: "id", kind: "bigint", nullable: false, codec: undefined },
    { name: "name", kind: "text", nullable: false, codec: undefined },
  ],
});

const byName: Template = sqlTemplate({
  text: 'SELECT "u"."id" AS "id", "u"."name" AS "name" FROM "users" AS "u" WHERE ("u"."name" = $1)',
  tables: [users],
  params: [{ name: "name", nullable: false, table: 0, column: "name" }],
  columns: [
    { name: "id", nullable: false, table: 0, column: "id" },
    { name: "name", nullable: false, table: 0, column: "name" },
  ],
});

const rows = (values: ReadonlyArray<ReadonlyArray<string | null>>, oids = [20, 25]) =>
  Promise.resolve({
    rows: values,
    fields: oids.map((dataTypeID, i) => ({ name: `c${i}`, dataTypeID })),
    rowCount: values.length,
  });

test("a value travels as a placeholder, rows decode by kind", async () => {
  const { pool, calls, log } = fakePool(() => rows([["7", "x'; DROP TABLE users; --"]]));
  const db = databaseOf(pool);
  const query = sqlBind(byName, { name: "'; DROP TABLE users; --" });
  const result = await Effect.runPromise(db.run_query(query));
  assert.deepEqual(result, [{ id: 7n, name: "x'; DROP TABLE users; --" }]);
  assert.equal(calls[0]?.text, sqlText(query));
  assert.deepEqual(calls[0]?.values, ["'; DROP TABLE users; --"]);
  assert.deepEqual(log, ["connect", "released"]);
});

test("a column of another type is a DecodeError", async () => {
  const { pool } = fakePool(() => rows([["seven", "x"]], [25, 25]));
  const exit = await Effect.runPromiseExit(databaseOf(pool).run_query(sqlBind(byName, { name: "x" })));
  assert.deepEqual(errorsOf(exit), [
    { _tag: errorsOf(exit)[0]?._tag, _0: { column: "id", message: "expected a bigint column, the database sent type 25" } },
  ]);
});

test("a refused query is a DbError with inert data, and the connection is reused", async () => {
  const failure = Object.assign(new Error("new row violates check constraint"), {
    code: "23514",
    constraint: "balance_not_negative",
  });
  const { pool, log } = fakePool(() => Promise.reject(failure));
  const exit = await Effect.runPromiseExit(databaseOf(pool).run_query(sqlBind(byName, { name: "x" })));
  const [error] = errorsOf(exit);
  assert.ok(error !== undefined && "code" in error._0);
  assert.equal(error._0.code, "23514");
  assert.deepEqual(error._0.constraint, Option.some("balance_not_negative"));
  assert.deepEqual(log, ["connect", "released"]);
});

test("an interrupted query closes its connection before the interruption ends", async () => {
  const { pool, log } = fakePool(() => new Promise(() => {}));
  const exit = await Effect.runPromise(
    Effect.gen(function* () {
      const fiber = yield* Effect.forkDetach(databaseOf(pool).run_query(sqlBind(byName, { name: "x" })));
      yield* Effect.sleep(10);
      yield* Fiber.interrupt(fiber);
      log.push("interrupted");
      return yield* Fiber.await(fiber);
    }),
  );
  assert.ok(Exit.hasInterrupts(exit));
  assert.deepEqual(log, ["connect", "destroying", "closed", "interrupted"]);
});

test("nullable parameters pass NULL, decimals pass as text", async () => {
  const accounts = sqlTable({
    name: "accounts",
    columns: [{ name: "balance", kind: "numeric", nullable: true, codec: undefined }],
  });
  const insert = sqlTemplate({
    text: 'INSERT INTO "accounts" ("balance") VALUES ($1)',
    tables: [accounts],
    params: [{ name: "balance", nullable: true, table: 0, column: "balance" }],
    columns: [],
  });
  const { pool, calls } = fakePool(() => rows([], []));
  const db = databaseOf(pool);
  await Effect.runPromise(db.run_execute(sqlBind(insert, { balance: Option.none() })));
  await Effect.runPromise(db.run_execute(sqlBind(insert, { balance: Option.some(BigDecimal.make(1250n, 2)) })));
  assert.deepEqual(
    calls.map((c) => c.values),
    [[null], ["12.5"]],
  );
});

test("raw SQL takes identifiers only checked and values only as placeholders", async () => {
  assert.equal(sqlIdentifier("users"), "users");
  assert.equal(sqlIdentifier("users; drop"), undefined);
  const raw = sqlRawQuery(
    [sqlRawText("SELECT name FROM "), sqlRawIdent("users"), sqlRawText(" WHERE name = "), sqlRawParam(codecString, "'; --")],
    codecString,
  );
  assert.equal(sqlText(raw), 'SELECT name FROM "users" WHERE name = $1');
  const { pool, calls } = fakePool(() => rows([], [25]));
  await Effect.runPromise(databaseOf(pool).run_execute(raw));
  assert.deepEqual(calls[0]?.values, ["'; --"]);
});
