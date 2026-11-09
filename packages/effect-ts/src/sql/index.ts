// `@flick-lang/effect-ts/sql`: tables and templates as the compiler emits
// them, binding without I/O, raw SQL with identifiers on their own channel,
// and the PostgreSQL pool behind `Database.pg`.
import { type Result, Schema } from "effect";

import { codecTransformResult } from "../schema/codecs.ts";
import type { SchemaError } from "../schema/issues.ts";
import type { AnySchema } from "../schema/schemas.ts";
import type { Column } from "./values.ts";

export { DatabaseService, databaseOf, pgLayer, pgTransaction } from "./pg.ts";
export type { Cancel } from "./pg.ts";
export type { PgClient, PgPool } from "./pg.ts";

/** What a `sql table` block compiles to. */
export interface Table {
  readonly name: string;
  readonly columns: ReadonlyArray<Column>;
}

/** A parameter or result column of a template. */
export interface Value {
  readonly name: string;
  readonly nullable: boolean;
  /** How the runtime passes a value no column decides: `kind` is a word of Flick. */
  readonly sql_kind?: string;
  readonly table?: number;
  readonly column?: string;
  readonly value?: unknown;
}

/** A checked query with its placeholders not bound yet. */
export interface Template {
  readonly text: string;
  readonly tables: ReadonlyArray<Table>;
  readonly params: ReadonlyArray<Value>;
  readonly columns: ReadonlyArray<Value>;
}

/** A query with its parameters bound, or raw SQL with its decoder. */
export type Query =
  | { readonly _tag: "Typed"; readonly template: Template; readonly values: Readonly<Record<string, unknown>> }
  | {
      readonly _tag: "Raw";
      readonly text: string;
      readonly values: ReadonlyArray<RawParam>;
      readonly decoder: AnySchema;
    };

/** A value of raw SQL, encoded to JSON by its codec. */
export interface RawParam {
  readonly _tag: "Param";
  readonly codec: AnySchema;
  readonly value: unknown;
}

/** One piece of raw SQL. */
export type Raw = { readonly _tag: "Text"; readonly text: string } | RawParam;

const frozen = <A extends object>(a: A): A => Object.freeze(a);

export const sqlTable = (table: { name: string; columns: ReadonlyArray<Column> }): Table =>
  frozen({ name: table.name, columns: frozen(table.columns.map((c) => frozen({ ...c }))) });

// a placeholder `$n`, or a quoted identifier, which may hold `$` itself
const PLACEHOLDER = /"(?:[^"]|"")*"|\$(\d+)/g;

const renumber = (text: string, slot: (n: number) => string): string =>
  text.replace(PLACEHOLDER, (whole, n: string | undefined) => (n === undefined ? whole : slot(Number(n))));

/**
 * What a `sql { … }` block compiles to. A template of the program used as a
 * subquery (`sql_kind` `"template"`) is spliced in its place: its text with
 * its placeholders after those before it, its tables and values appended.
 */
export const sqlTemplate = (template: Template): Template => {
  if (!template.params.some((p) => p.sql_kind === "template")) return frozen(template);
  const tables: Array<Table> = [...template.tables];
  const params: Array<Value> = [];
  const texts: Array<string> = [];
  for (const p of template.params) {
    if (p.sql_kind !== "template") {
      params.push(p);
      texts.push(`$${params.length}`);
      continue;
    }
    const sub = p.value as Template;
    const [base, shift] = [params.length, tables.length];
    tables.push(...sub.tables);
    for (const q of sub.params) params.push(q.table === undefined ? q : frozen({ ...q, table: q.table + shift }));
    texts.push(renumber(sub.text, (n) => `$${base + n}`));
  }
  const text = renumber(template.text, (n) => texts[n - 1] ?? `$${n}`);
  return frozen({ ...template, text, tables: frozen(tables), params: frozen(params) });
};

/** `Sql.Derive.column name kind nullable codec`: a column as a table holds it. */
export const sqlColumn = (name: string, kind: string, nullable: boolean, codec: unknown): unknown =>
  frozen({ name, kind, nullable, codec });

/** `Sql.codec decode encode`: over the value of the column's SQL type. */
export const sqlCodec = (
  decode: (value: unknown) => Result.Result<unknown, SchemaError>,
  encode: (value: unknown) => unknown,
): AnySchema => codecTransformResult(decode, encode, Schema.Unknown);

export const sqlBind = (template: Template, values: Readonly<Record<string, unknown>>): Query =>
  frozen({ _tag: "Typed", template, values });

export const sqlText = (query: Query): string => (query._tag === "Typed" ? query.template.text : query.text);

export const sqlTemplateText = (template: Template): string => template.text;

export const sqlParameterCount = (query: Query): number =>
  query._tag === "Typed" ? query.template.params.length : query.values.length;

const IDENTIFIER = /^[A-Za-z_][A-Za-z0-9_]*$/;

/** A checked identifier, or `undefined`. */
export const sqlIdentifier = (name: string): string | undefined =>
  IDENTIFIER.test(name) && new TextEncoder().encode(name).length <= 63 ? name : undefined;

export const sqlRawText = (text: string): Raw => frozen({ _tag: "Text", text });

export const sqlRawIdent = (name: string): Raw => sqlRawText(`"${name.replaceAll('"', '""')}"`);

export const sqlRawParam = (codec: AnySchema, value: unknown): Raw => frozen({ _tag: "Param", codec, value });

/** Joins the pieces; each parameter becomes the next placeholder. */
export const sqlRawQuery = (pieces: ReadonlyArray<Raw>, decoder: AnySchema): Query => {
  const values: Array<RawParam> = [];
  let text = "";
  for (const piece of pieces) {
    if (piece._tag === "Text") text += piece.text;
    else {
      values.push(piece);
      text += `$${values.length}`;
    }
  }
  return frozen({ _tag: "Raw", text, values, decoder });
};
