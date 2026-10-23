// The PostgreSQL pool behind `Database.pg`. A query borrows a connection
// for its whole run; when it is interrupted the query is not rolled back,
// and the connection is closed, never handed back to the pool, before the
// interruption completes. The pool ends with the scope of the `provide`.
import { type Context, Effect, Exit, Layer, Option, Result } from "effect";

import { codecDecodeResult, codecEncodeResult } from "../schema/index.ts";
import type { Query, Template, Value } from "./index.ts";
import {
  type Column,
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
}

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
    ? { name: v.name, kind: v.kind ?? "text", nullable: v.nullable, codec: undefined }
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

/** Runs on a borrowed connection; an interrupted run closes it. */
const run = (pool: PgPool, text: string, values: ReadonlyArray<unknown>): Effect.Effect<PgResult, DbError> =>
  Effect.acquireUseRelease(
    Effect.tryPromise({ try: () => pool.connect(), catch: dbError }),
    (client) =>
      Effect.callback<PgResult, DbError>((resume) => {
        client.query({ text, values, rowMode: "array" }).then(
          (r) => resume(Effect.succeed(r)),
          (e: unknown) => resume(Effect.fail(dbError(e))),
        );
      }),
    (client, exit) =>
      Exit.hasInterrupts(exit)
        ? Effect.promise(
            () =>
              new Promise<void>((done) => {
                client.once("end", done);
                client.release(new Error("query interrupted"));
              }),
          )
        : Effect.sync(() => {
            // a failure without SQLSTATE is the connection's: do not reuse it
            const broken = Exit.isFailure(exit) && !Exit.hasInterrupts(exit) && failedWithoutCode(exit);
            client.release(broken ? new Error("connection failed") : undefined);
          }),
  );

const failedWithoutCode = (exit: Exit.Exit<unknown, DbError>): boolean =>
  Exit.isFailure(exit) && exit.cause.reasons.some((r) => r._tag === "Fail" && r.error._0.code === "");

const fromResult = <A, E>(r: Result.Result<A, E>): Effect.Effect<A, E> =>
  Result.isFailure(r) ? Effect.fail(r.failure) : Effect.succeed(r.success);

/** The implementation of `Database.Database` over a pool. */
export const databaseOf = (pool: PgPool) => {
  const execute = (query: Query): Effect.Effect<PgResult, Failure> =>
    Effect.flatMap(fromResult(encode(query)), (values) =>
      run(pool, query._tag === "Typed" ? query.template.text : query.text, values),
    );
  return Object.freeze({
    run_query: (query: Query): Effect.Effect<ReadonlyArray<unknown>, Failure> =>
      Effect.flatMap(execute(query), (result) =>
        fromResult(query._tag === "Typed" ? decodeTyped(query.template, result) : decodeRaw(query, result)),
      ),
    run_execute: (query: Query): Effect.Effect<number, Failure> =>
      Effect.map(execute(query), (result) => result.rowCount ?? 0),
  });
};

interface PgConfig {
  readonly connection_string: string;
  readonly max_connections: number;
}

type Database = ReturnType<typeof databaseOf>;

const asText = (value: string): string => value;

/** A pool whose values all arrive as text, which `values.ts` decodes. */
const makePool = async (config: PgConfig): Promise<PgPool> => {
  const pg = await import("pg");
  return new pg.default.Pool({
    connectionString: config.connection_string,
    max: config.max_connections,
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
