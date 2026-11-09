import assert from "node:assert/strict";
import { test } from "node:test";

import { BigDecimal, Effect, Exit, Fiber, Option } from "effect";

import {
  type Cancel,
  DatabaseService,
  type PgClient,
  type PgPool,
  type Template,
  databaseOf,
  pgTransaction,
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

test("a template used as a subquery is spliced, its placeholders and tables after those before it", async () => {
  const accounts = sqlTable({
    name: "a$1",
    columns: [{ name: "owner", kind: "bigint", nullable: false, codec: undefined }],
  });
  const owners = sqlTemplate({
    text: 'SELECT "a"."owner" AS "owner" FROM "a$1" AS "a" WHERE ("a"."owner" > $1)',
    tables: [accounts],
    params: [{ name: "$min", nullable: false, table: 0, column: "owner", value: 3n }],
    columns: [{ name: "owner", nullable: false, table: 0, column: "owner" }],
  });
  const outer = sqlTemplate({
    text: 'SELECT "u"."id" AS "id" FROM "users" AS "u" WHERE (("u"."name" = $1) AND ("u"."id" IN ($2))) LIMIT $3',
    tables: [users],
    params: [
      { name: "name", nullable: false, table: 0, column: "name" },
      { name: "$owners", nullable: false, sql_kind: "template", value: owners },
      { name: "n", nullable: false, sql_kind: "number" },
    ],
    columns: [{ name: "id", nullable: false, table: 0, column: "id" }],
  });
  assert.equal(
    outer.text,
    'SELECT "u"."id" AS "id" FROM "users" AS "u" WHERE (("u"."name" = $1) AND ("u"."id" IN (SELECT "a"."owner" AS "owner" FROM "a$1" AS "a" WHERE ("a"."owner" > $2)))) LIMIT $3',
  );
  assert.deepEqual(
    outer.params.map((p) => [p.name, p.table]),
    [["name", 0], ["$min", 1], ["n", undefined]],
  );
  assert.deepEqual(outer.tables.map((t) => t.name), ["users", "a$1"]);
  const { pool, calls } = fakePool(() => rows([["7"]], [20]));
  await Effect.runPromise(databaseOf(pool).run_query(sqlBind(outer, { name: "Ada", n: 5 })));
  assert.deepEqual(calls[0]?.values, ["Ada", "3", 5]);
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
  const started = Promise.withResolvers<void>();
  const { pool, log } = fakePool(() => {
    started.resolve();
    return Promise.withResolvers<never>().promise;
  });
  const exit = await Effect.runPromise(
    Effect.gen(function* () {
      const fiber = yield* Effect.forkDetach(databaseOf(pool).run_query(sqlBind(byName, { name: "x" })));
      yield* Effect.promise(() => started.promise);
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

/** An answer with no rows, as BEGIN, COMMIT or an UPDATE give. */
const done = () => Promise.resolve({ rows: [], fields: [], rowCount: 1 });

/** Runs `body` in a transaction of `pool`, its database the transaction's. */
const transaction = <A, E, R>(pool: PgPool, body: Effect.Effect<A, E, R>, cancel?: Cancel) =>
  Effect.provideService(pgTransaction(body), DatabaseService, databaseOf(pool, cancel));

const lookup = Effect.flatMap(Effect.service(DatabaseService), (db) => db.run_query(sqlBind(byName, { name: "x" })));

test("a transaction runs its queries on one connection, then commits", async () => {
  const { pool, log, calls } = fakePool((call) => (call.text.startsWith("SELECT") ? rows([["1", "x"]]) : done()));
  const exit = await Effect.runPromiseExit(transaction(pool, Effect.andThen(lookup, lookup)));
  assert.ok(Exit.isSuccess(exit), String(exit));
  assert.deepEqual(calls.map((c) => c.text.split(" ")[0]), ["BEGIN", "SELECT", "SELECT", "COMMIT"]);
  assert.deepEqual(log, ["connect", "released"]);
});

test("a failed query rolls the transaction back", async () => {
  const violation = Object.assign(new Error("check"), { code: "23514" });
  let selects = 0;
  const { pool, calls } = fakePool((call) => {
    if (!call.text.startsWith("SELECT")) return done();
    selects += 1;
    return selects === 2 ? Promise.reject(violation) : rows([["1", "x"]]);
  });
  const exit = await Effect.runPromiseExit(transaction(pool, Effect.andThen(lookup, lookup)));
  const [error] = errorsOf(exit);
  assert.ok(error !== undefined && "code" in error._0);
  assert.equal(error._0.code, "23514");
  assert.deepEqual(calls.map((c) => c.text.split(" ")[0]), ["BEGIN", "SELECT", "SELECT", "ROLLBACK"]);
});

test("a nested transaction is a savepoint whose failure the outer one survives", async () => {
  const { pool, calls } = fakePool((call) => (call.text.startsWith("SELECT") ? rows([["1", "x"]]) : done()));
  const outer = Effect.gen(function* () {
    const nested = yield* Effect.service(DatabaseService);
    // the inner transaction runs on the outer's database: a savepoint
    yield* Effect.exit(Effect.provideService(pgTransaction(Effect.andThen(lookup, Effect.fail("no"))), DatabaseService, nested));
    return yield* lookup;
  });
  const exit = await Effect.runPromiseExit(transaction(pool, outer));
  assert.ok(Exit.isSuccess(exit), String(exit));
  assert.deepEqual(
    calls.map((c) => c.text),
    [
      "BEGIN",
      "SAVEPOINT flick_savepoint_1",
      byName.text,
      "ROLLBACK TO SAVEPOINT flick_savepoint_1",
      "RELEASE SAVEPOINT flick_savepoint_1",
      byName.text,
      "COMMIT",
    ],
  );
});

test("an interrupted query is cancelled on the server before its connection closes", async () => {
  const started = Promise.withResolvers<void>();
  const pending = Promise.withResolvers<never>();
  const { pool, log } = fakePool(() => {
    started.resolve();
    return pending.promise;
  });
  const cancel: Cancel = () => {
    log.push("cancel");
    pending.reject(Object.assign(new Error("canceling statement due to user request"), { code: "57014" }));
    return Promise.resolve(true);
  };
  const exit = await Effect.runPromise(
    Effect.gen(function* () {
      const fiber = yield* Effect.forkDetach(databaseOf(pool, cancel).run_query(sqlBind(byName, { name: "x" })));
      yield* Effect.promise(() => started.promise);
      yield* Fiber.interrupt(fiber);
      log.push("interrupted");
      return yield* Fiber.await(fiber);
    }),
  );
  assert.ok(Exit.hasInterrupts(exit));
  assert.deepEqual(log, ["connect", "cancel", "destroying", "closed", "interrupted"]);
});

test("a commit whose answer is lost is commit_unknown", async () => {
  const { pool } = fakePool((call) => (call.text === "COMMIT" ? Promise.reject(new Error("Connection terminated")) : done()));
  const exit = await Effect.runPromiseExit(transaction(pool, Effect.void));
  const [error] = errorsOf(exit);
  assert.ok(error !== undefined && "code" in error._0);
  assert.equal(error._0.code, "commit_unknown");
});
