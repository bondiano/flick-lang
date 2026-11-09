// The PostgreSQL pool behind `Database.pg`. A query borrows a connection
// for its whole run, a transaction for all of its queries. An interrupted
// query is cancelled on the server, through the protocol's CancelRequest,
// and its connection is closed, never handed back to the pool, before the
// interruption completes; what it wrote is not rolled back, unless it ran in
// a transaction, which then ends. The pool ends with the scope of the
// `provide`.
import { Context, Effect, Exit, Layer, Option, Result } from "effect";

import { codecDecodeResult, codecEncodeResult } from "../schema/index.ts";
import type { Query, Template, Value } from "./index.ts";
import {
  type Column,
  DB_ERROR,
  type DbError,
  type DecodeError,
  type EncodeError,
  acceptsOid,
  dbError,
  decodeColumn,
  decodeError,
  decodeKind,
  encodeColumn,
  encodeError,
} from "./values.ts";

/** The part of a `pg` client the adapter uses. */
export interface PgClient {
  query(config: {
    readonly text: string;
    readonly values: ReadonlyArray<unknown>;
    readonly rowMode: "array";
  }): Promise<PgResult>;
  /** With an error the pool closes the connection instead of keeping it. */
  release(destroy?: Error): void;
  once(event: "end", listener: () => void): unknown;
  /** The backend's key, which a CancelRequest names; set once connected. */
  readonly processID?: number | null;
  readonly secretKey?: number | null;
  readonly host?: string;
  readonly port?: number;
}

/** Asks the server to cancel what `client` runs; whether a request was sent. */
export type Cancel = (client: PgClient) => Promise<boolean>;

/** CancelRequest: its length, its code, then the backend's process and key. */
const CANCEL_CODE = 80877102;

/**
 * The protocol's cancel: a request on a connection of its own, which takes
 * no backend of the pool.
 */
// shortcut: plain TCP only; a server that requires TLS for it ignores the request, and the query ends at its timeout
const cancelRequest: Cancel = async (client) => {
  const { processID: pid, secretKey: key } = client;
  if (pid === undefined || pid === null || key === undefined || key === null) return false;
  const { connect } = await import("node:net");
  return new Promise<boolean>((done) => {
    const socket = connect({ host: client.host ?? "localhost", port: client.port ?? 5432 });
    const finish = (): void => {
      socket.destroy();
      done(true);
    };
    socket.once("error", finish);
    socket.once("close", finish);
    socket.once("connect", () => {
      const request = Buffer.alloc(16);
      request.writeInt32BE(16, 0);
      request.writeInt32BE(CANCEL_CODE, 4);
      request.writeInt32BE(pid, 8);
      request.writeInt32BE(key, 12);
      socket.end(request);
    });
  });
};

/** How long an interruption waits for a cancelled query to stop. */
const CANCEL_WAIT = 5000;

export interface PgResult {
  readonly rows: ReadonlyArray<ReadonlyArray<string | null>>;
  readonly fields: ReadonlyArray<{ readonly name: string; readonly dataTypeID: number }>;
  readonly rowCount: number | null;
}

/** The part of a `pg` pool the adapter uses. */
export interface PgPool {
  connect(): Promise<PgClient>;
  end(): Promise<void>;
}

type Failure = EncodeError | DbError | DecodeError;

const columnOf = (template: Template, v: Value): Column =>
  v.table === undefined
    ? { name: v.name, kind: v.sql_kind ?? "text", nullable: v.nullable, codec: undefined }
    : (template.tables[v.table]?.columns.find((c) => c.name === v.column) ?? {
        name: v.name,
        kind: "text",
        nullable: v.nullable,
        codec: undefined,
      });

/** The placeholder values of a query. */
const encode = (query: Query): Result.Result<ReadonlyArray<unknown>, EncodeError> => {
  const out: Array<unknown> = [];
  if (query._tag === "Raw") {
    for (const [i, piece] of query.values.entries()) {
      const json = codecEncodeResult(piece.codec, piece.value);
      if (Result.isFailure(json)) return Result.fail(encodeError(`$${i + 1}`, "its codec refused the value"));
      const v = json.success;
      out.push(v === null || typeof v !== "object" ? v : JSON.stringify(v));
    }
    return Result.succeed(out);
  }
  for (const p of query.template.params) {
    let value = "value" in p ? p.value : query.values[p.name];
    if (p.nullable) {
      if (!Option.isOption(value) || Option.isNone(value)) {
        out.push(null);
        continue;
      }
      value = value.value;
    }
    const encoded = encodeColumn(columnOf(query.template, p), value);
    if (Result.isFailure(encoded)) return Result.fail(encodeError(p.name, encoded.failure));
    out.push(encoded.success);
  }
  return Result.succeed(out);
};

/** The rows of a typed query, as records of its results. */
const decodeTyped = (template: Template, result: PgResult): Result.Result<ReadonlyArray<unknown>, DecodeError> => {
  const columns = template.columns.map((c) => ({ value: c, column: columnOf(template, c) }));
  for (const [i, { value, column }] of columns.entries()) {
    const oid = result.fields[i]?.dataTypeID ?? -1;
    if (!acceptsOid(column.kind, oid)) {
      return Result.fail(decodeError(value.name, `expected a ${column.kind} column, the database sent type ${oid}`));
    }
  }
  const rows: Array<unknown> = [];
  for (const raw of result.rows) {
    const row: Record<string, unknown> = {};
    for (const [i, { value, column }] of columns.entries()) {
      const text = raw[i] ?? null;
      if (text === null) {
        if (!value.nullable) return Result.fail(decodeError(value.name, "unexpected NULL"));
        row[value.name] = Option.none();
        continue;
      }
      const decoded = decodeColumn(column, text);
      if (Result.isFailure(decoded)) return Result.fail(decodeError(value.name, decoded.failure));
      row[value.name] = value.nullable ? Option.some(decoded.success) : decoded.success;
    }
    rows.push(Object.freeze(row));
  }
  return Result.succeed(rows);
};

const RAW_KINDS: Readonly<Record<number, string>> = { 16: "bool", 21: "number", 23: "number", 700: "number", 701: "number", 114: "json", 3802: "json" };

/** The rows of a raw query, each object through the decoder. */
const decodeRaw = (query: Query & { _tag: "Raw" }, result: PgResult): Result.Result<ReadonlyArray<unknown>, DecodeError> => {
  const rows: Array<unknown> = [];
  for (const raw of result.rows) {
    const row: Record<string, unknown> = {};
    for (const [i, field] of result.fields.entries()) {
      const text = raw[i] ?? null;
      const kind = RAW_KINDS[field.dataTypeID] ?? "text";
      const value = text === null ? Result.succeed(null) : decodeKind(kind, text);
      if (Result.isFailure(value)) return Result.fail(decodeError(field.name, value.failure));
      row[field.name] = value.success;
    }
    const decoded = codecDecodeResult(query.decoder, row);
    if (Result.isFailure(decoded)) {
      return Result.fail(decodeError("", decoded.failure._0.map((i) => `${i.path}: ${i.message}`).join("; ")));
    }
    rows.push(decoded.success);
  }
  return Result.succeed(rows);
};

/**
 * Runs on `client`. Interrupted, it cancels the query on the server and
 * waits a while for it to stop, so the backend is free when the
 * interruption completes.
 */
const onClient = (client: PgClient, cancel: Cancel, text: string, values: ReadonlyArray<unknown>) =>
  Effect.callback<PgResult, DbError>((resume) => {
    const running = client.query({ text, values, rowMode: "array" });
    running.then(
      (r) => resume(Effect.succeed(r)),
      (e: unknown) => resume(Effect.fail(dbError(e))),
    );
    return Effect.promise(async () => {
      if (!(await cancel(client))) return;
      const stopped = running.then(
        () => undefined,
        () => undefined,
      );
      await Promise.race([stopped, new Promise((done) => setTimeout(done, CANCEL_WAIT))]);
    });
  });

const connected = (pool: PgPool): Effect.Effect<PgClient, DbError> =>
  Effect.tryPromise({ try: () => pool.connect(), catch: dbError });

/**
 * Hands `client` back after `exit`: closed when it was interrupted, or when
 * it failed without SQLSTATE, a failure of the connection.
 */
const handBack = (client: PgClient, exit: Exit.Exit<unknown, unknown>): Effect.Effect<void> =>
  Exit.hasInterrupts(exit)
    ? Effect.promise(
        () =>
          new Promise<void>((done) => {
            client.once("end", done);
            client.release(new Error("query interrupted"));
          }),
      )
    : Effect.sync(() => {
        const broken = Exit.isFailure(exit) && failedWithoutCode(exit);
        client.release(broken ? new Error("connection failed") : undefined);
      });

/** Runs on a borrowed connection; an interrupted run cancels and closes it. */
const run = (pool: PgPool, cancel: Cancel, text: string, values: ReadonlyArray<unknown>) =>
  Effect.acquireUseRelease(connected(pool), (client) => onClient(client, cancel, text, values), handBack);

const failedWithoutCode = (exit: Exit.Exit<unknown, unknown>): boolean =>
  Exit.isFailure(exit) &&
  exit.cause.reasons.some((r) => r._tag === "Fail" && isDbError(r.error) && r.error._0.code === "");

const isDbError = (error: unknown): error is DbError =>
  typeof error === "object" && error !== null && Reflect.get(error, "_tag") === DB_ERROR;

const fromResult = <A, E>(r: Result.Result<A, E>): Effect.Effect<A, E> =>
  Result.isFailure(r) ? Effect.fail(r.failure) : Effect.succeed(r.success);

/** The service `Database.Database`, which a transaction provides to its body. */
export const DatabaseService = /*#__PURE__*/ Context.Service<Database>("Database.Database");

/** Marks the implementations of this adapter, which run transactions. */
const TRANSACTION = /*#__PURE__*/ Symbol.for("@flick-lang/runtime/Database.transaction");

type Runner = (text: string, values: ReadonlyArray<unknown>) => Effect.Effect<PgResult, DbError>;

type Transaction = <A, E, R>(body: Effect.Effect<A, E, R>) => Effect.Effect<A, E | DbError, Exclude<R, Database>>;

/** `Database.Database` running each query with `runner`. */
const implementation = (runner: Runner, transaction: Transaction) => {
  const execute = (query: Query): Effect.Effect<PgResult, Failure> =>
    Effect.flatMap(fromResult(encode(query)), (values) =>
      runner(query._tag === "Typed" ? query.template.text : query.text, values),
    );
  return Object.freeze({
    run_query: (query: Query): Effect.Effect<ReadonlyArray<unknown>, Failure> =>
      Effect.flatMap(execute(query), (result) =>
        fromResult(query._tag === "Typed" ? decodeTyped(query.template, result) : decodeRaw(query, result)),
      ),
    run_execute: (query: Query): Effect.Effect<number, Failure> =>
      Effect.map(execute(query), (result) => result.rowCount ?? 0),
    [TRANSACTION]: transaction,
  });
};

type Database = ReturnType<typeof implementation>;

/** A COMMIT whose answer was lost: the transaction may or may not have happened. */
const commitUnknown = (error: DbError): DbError =>
  error._0.code === ""
    ? { ...error, _0: { ...error._0, code: "commit_unknown", message: `the commit's outcome is unknown: ${error._0.message}` } }
    : error;

/**
 * `body` with the database of `client`, between `open` and `close` on
 * success or `undo` on failure. An interruption ends the transaction with
 * its connection, which the server rolls back.
 */
const bracketed = <A, E, R>(
  client: PgClient,
  cancel: Cancel,
  depth: number,
  body: Effect.Effect<A, E, R>,
  [open, close, undo]: readonly [string, string, ReadonlyArray<string>],
): Effect.Effect<A, E | DbError, Exclude<R, Database>> => {
  const on = (text: string) => onClient(client, cancel, text, []);
  return Effect.gen(function* () {
    yield* on(open);
    const exit = yield* Effect.exit(Effect.provideService(body, DatabaseService, pinned(client, cancel, depth + 1)));
    if (Exit.isSuccess(exit)) {
      // the commit waits for its answer: an interruption could not tell whether it happened
      yield* Effect.uninterruptible(Effect.mapError(on(close), depth === 0 ? commitUnknown : (e) => e));
      return exit.value;
    }
    for (const text of undo) yield* on(text);
    return yield* exit;
  });
};

/** The database of a transaction's connection; a nested one is a savepoint. */
const pinned = (client: PgClient, cancel: Cancel, depth: number): Database =>
  implementation(
    (text, values) => onClient(client, cancel, text, values),
    (body) => {
      const name = `flick_savepoint_${depth}`;
      return bracketed(client, cancel, depth, body, [
        `SAVEPOINT ${name}`,
        `RELEASE SAVEPOINT ${name}`,
        [`ROLLBACK TO SAVEPOINT ${name}`, `RELEASE SAVEPOINT ${name}`],
      ]);
    },
  );

/** The implementation of `Database.Database` over a pool. */
export const databaseOf = (pool: PgPool, cancel: Cancel = cancelRequest): Database =>
  implementation(
    (text, values) => run(pool, cancel, text, values),
    (body) =>
      Effect.acquireUseRelease(
        connected(pool),
        (client) => bracketed(client, cancel, 0, body, ["BEGIN", "COMMIT", ["ROLLBACK"]]),
        handBack,
      ),
  );

/** Whether `database` is this adapter's, which runs transactions. */
const hasTransaction = (database: object): database is Database => TRANSACTION in database;

/**
 * `Database.transaction body`: `body` on one connection, committed when it
 * succeeds and rolled back when it fails; inside another, a savepoint.
 */
export const pgTransaction = <A, E, R>(
  body: Effect.Effect<A, E, R>,
): Effect.Effect<A, E | DbError, Exclude<R, Database> | Database> =>
  Effect.flatMap(Effect.service(DatabaseService), (database): Effect.Effect<A, E | DbError, Exclude<R, Database>> => {
    if (!hasTransaction(database)) return Effect.die(new Error("this `Database` runs no transactions"));
    return database[TRANSACTION](body);
  });

interface PgConfig {
  readonly connection_string: string;
  readonly max_connections: number;
  readonly statement_timeout_ms: Option.Option<number>;
  readonly acquire_timeout_ms: Option.Option<number>;
}

const asText = (value: string): string => value;

/** A pool whose values all arrive as text, which `values.ts` decodes. */
const makePool = async (config: PgConfig): Promise<PgPool> => {
  const pg = await import("pg");
  return new pg.default.Pool({
    connectionString: config.connection_string,
    max: config.max_connections,
    // the server ends a statement that runs longer
    statement_timeout: Option.getOrUndefined(config.statement_timeout_ms),
    // a query waits at most this long for a connection of the pool
    connectionTimeoutMillis: Option.getOrElse(config.acquire_timeout_ms, () => 0),
    types: { getTypeParser: () => asText },
  });
};

/** `Database.pg config`: a pool built with the layer and ended with its scope. */
export const pgLayer = <I>(tag: Context.Key<I, Database>, config: PgConfig): Layer.Layer<I> =>
  Layer.effect(
    tag,
    Effect.map(
      Effect.acquireRelease(
        Effect.promise(() => makePool(config)),
        (pool) => Effect.promise(() => pool.end()),
      ),
      databaseOf,
    ),
  );
