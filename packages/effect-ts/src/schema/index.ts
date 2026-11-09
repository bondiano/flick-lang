// The source API of schemas and codecs over Effect Schema: decoding and
// encoding with the program's `SchemaError`, the Standard Schema adapter,
// the `codec` DSL and refined types.
import { Cause, type Context, Effect, Exit, type Result, Schema, type SchemaAST } from "effect";

import { err, ok } from "../data.ts";
import { CurrentOwner } from "../owner.ts";
import { registerCodec } from "./aot.ts";
import {
  type Field,
  type FieldSpec,
  codecCheck,
  codecFields,
  codecTransformAs as transformAs,
  codecTransformEffect as transformEffect,
  codecTransformPure as transformPure,
  codecTransformResult as transformResult,
} from "./codecs.ts";
import { type ExportError, exportJsonSchema } from "./exporter.ts";
import { type SchemaError, isSchemaError, requireInert, schemaError, toSchemaError } from "./issues.ts";
import { erase } from "@flick-lang/runtime/internal";
import { noteOf, notesOf } from "./notes.ts";
import { type AnySchema, jsonOf, schemaRefined } from "./schemas.ts";

export {
  codecBigint,
  codecBool,
  codecIgnoreExtra,
  codecJson,
  codecList,
  codecInt,
  codecNumberFinite,
  codecNumberLossless,
  codecOptionNull,
  codecOptionNullish,
  codecRedactedMasked,
  codecRedactedReveal,
  codecString,
  codecTaggedOption,
} from "./codecs.ts";
export {
  schemaAdt,
  schemaBigint,
  schemaBool,
  schemaLazy,
  schemaList,
  schemaNever,
  schemaInt,
  schemaNumber,
  schemaOpaque,
  schemaOption,
  schemaRecord,
  schemaRedacted,
  schemaRefined,
  schemaResult,
  schemaString,
  schemaTuple,
  schemaUnit,
  schemaUnknown,
  schemaWithGenerator,
} from "./schemas.ts";
export type { AnySchema, Variant } from "./schemas.ts";
export type { ExportError } from "./exporter.ts";
export type { Issue, SchemaError } from "./issues.ts";
export type { Field, FieldSpec } from "./codecs.ts";
export { enableSchemaAot, installSchemaAot, schemaCompilerRuntime, writeSchemaAot } from "./aot.ts";
export type { CompiledSchemas } from "./aot.ts";
export { schemaError };

/** Wire input is strict: a key no field names is an error. */
const OPTIONS: SchemaAST.ParseOptions = { onExcessProperty: "error", reportInput: true };
/** For inputs that carry more than any codec declares, such as HTTP headers. */
const LOOSE: SchemaAST.ParseOptions = { onExcessProperty: "ignore", reportInput: true };

const runtimeAsts = /*#__PURE__*/ new WeakMap<SchemaAST.AST, boolean>();

const childrenOf = (ast: SchemaAST.AST): ReadonlyArray<SchemaAST.AST> => {
  const links = (ast.encoding ?? []).map((link) => link.to);
  switch (ast._tag) {
    case "Objects":
      return [...links, ...ast.propertySignatures.map((p) => p.type), ...ast.indexSignatures.map((i) => i.type)];
    case "Arrays":
      return [...links, ...ast.elements, ...ast.rest];
    case "Union":
      return [...links, ...ast.types];
    case "Declaration":
      return [...links, ...ast.typeParameters];
    case "Suspend":
      return [...links, ast.thunk()];
    default:
      return links;
  }
};

/** Whether decoding with `ast` may need services or wait: a runtime transformation or default. */
const reaches = (ast: SchemaAST.AST, seen: Set<SchemaAST.AST | string>): boolean => {
  const notes = notesOf(ast);
  // a generic recursive type makes a new node per level: its name stands for all of them
  const key = notes["flick/ref"] ?? ast;
  if (seen.has(key)) return false;
  seen.add(key);
  if (noteOf(ast, "flick/transform") === "effect" || noteOf(ast, "flick/effect") === true) return true;
  return childrenOf(ast).some((child) => reaches(child, seen));
};

const needsRuntime = (ast: SchemaAST.AST): boolean => {
  let known = runtimeAsts.get(ast);
  if (known === undefined) {
    known = reaches(ast, new Set());
    runtimeAsts.set(ast, known);
  }
  return known;
};

/** A codec that decodes and encodes without services or waiting. */
type PureCodec = Schema.Codec<unknown>;

/** Whether `codec` is pure: no runtime transformation or default anywhere in it. */
const isPure = (codec: AnySchema): codec is AnySchema & PureCodec => !needsRuntime(codec.ast);

const fromExit = <A>(exit: Exit.Exit<A, Schema.SchemaError>, encoding: boolean): Exit.Exit<A, SchemaError> =>
  Exit.mapError(exit, toSchemaError(encoding));

/** The result of a pure codec, at once. */
const decodePure = (codec: PureCodec, input: unknown, options = OPTIONS): Exit.Exit<unknown, SchemaError> => {
  const problem = requireInert(input);
  if (problem) return Exit.fail(problem);
  // a pure codec decodes synchronously, through compiled decoders when installed
  return fromExit(Schema.decodeUnknownExit(codec)(input, options), false);
};

/** `Codec.decode codec input`: an effect failing with `SchemaError`. */
export const codecDecode = (
  codec: AnySchema,
  input: unknown,
  extra: "refuse" | "ignore" = "refuse",
): Effect.Effect<unknown, SchemaError, unknown> =>
  Effect.suspend(() => {
    const options = extra === "ignore" ? LOOSE : OPTIONS;
    if (isPure(codec)) return decodePure(codec, input, options);
    const problem = requireInert(input);
    if (problem) return Effect.fail(problem);
    return Effect.mapError(Schema.decodeUnknownEffect(codec)(input, options), toSchemaError(false));
  });

/** `Codec.encode codec value`. */
export const codecEncode = (codec: AnySchema, value: unknown): Effect.Effect<unknown, SchemaError, unknown> =>
  Effect.suspend(() =>
    isPure(codec)
      ? fromExit(Schema.encodeUnknownExit(codec)(value, OPTIONS), true)
      : Effect.mapError(Schema.encodeUnknownEffect(codec)(value, OPTIONS), toSchemaError(true)),
  );

const notPure = (): never => {
  throw new TypeError("a codec with runtime decoding is no PureCodec");
};

const resultOf = <A>(exit: Exit.Exit<A, SchemaError>): Result.Result<A, SchemaError> => {
  if (Exit.isSuccess(exit)) return ok(exit.value);
  const failure = exit.cause.reasons.find(Cause.isFailReason);
  if (failure) return err(failure.error);
  throw Cause.squash(exit.cause);
};

/** `Codec.decode_result`: a pure codec runs synchronously. */
export const codecDecodeResult = (codec: AnySchema, input: unknown): Result.Result<unknown, SchemaError> =>
  isPure(codec) ? resultOf(decodePure(codec, input)) : notPure();
export const codecEncodeResult = (codec: AnySchema, value: unknown): Result.Result<unknown, SchemaError> =>
  isPure(codec) ? resultOf(fromExit(Schema.encodeUnknownExit(codec)(value, OPTIONS), true)) : notPure();

/** `Schema.validate schema input`: whether `input` is a value of the type. */
export const schemaValidate = (schema: AnySchema, input: unknown): Result.Result<unknown, SchemaError> =>
  isPure(schema) ? resultOf(fromExit(Schema.decodeUnknownExit(schema)(input, OPTIONS), false)) : notPure();

/** `schemaValidate` reporting every issue, not the first: a form shows an error on each field. */
export const schemaValidateAll = (schema: AnySchema, input: unknown): Result.Result<unknown, SchemaError> =>
  isPure(schema)
    ? resultOf(fromExit(Schema.decodeUnknownExit(schema)(input, { ...OPTIONS, errors: "all" }), false))
    : notPure();

/** `Codec.json schema`: the canonical JSON codec, compiled when installed. */
export const schemaJsonCodec = (schema: AnySchema): AnySchema => registerCodec(jsonOf(schema));

export const dslField = (wire: string, codec: AnySchema): FieldSpec => Object.freeze({ wire, codec, optional: false });
export const dslOptionalField = (wire: string, codec: AnySchema): FieldSpec =>
  Object.freeze({ wire, codec, optional: true });
export const dslFieldEffect = (wire: string, codec: AnySchema): FieldSpec =>
  Object.freeze({ wire, codec, optional: false });
export const dslOnMissing = (factory: (unit: undefined) => unknown, field: FieldSpec): FieldSpec =>
  Object.freeze({ ...field, onMissing: factory });
export const dslOnMissingEffect = (
  factory: (unit: undefined) => Effect.Effect<unknown, SchemaError, unknown>,
  field: FieldSpec,
): FieldSpec => Object.freeze({ ...field, onMissingEffect: factory, codec: field.codec.annotate({ "flick/effect": true }) });

/**
 * The codec of a `codec` block: `fields` lists `[label, field]`. Two fields
 * sharing a wire name is a defect, when the compiler could not see it.
 */
export const codecBlock = (name: string, fields: ReadonlyArray<readonly [string, FieldSpec]>): AnySchema => {
  const wires = new Set<string>();
  for (const [, f] of fields) {
    if (wires.has(f.wire)) throw new Error(`two fields of the codec of ${name} use the wire name "${f.wire}"`);
    wires.add(f.wire);
  }
  const labelled = fields.map(([label, f]): Field => Object.freeze({ label, ...f }));
  return registerCodec(codecFields(name, labelled, false));
};

/** The fields of a `codec` block, in the written order. */
type BlockFields = ReadonlyArray<readonly [string, FieldSpec]>;

/** `Codec.Derive.fields`. */
export const codecFieldsEmpty: BlockFields = Object.freeze([]);

/** `Codec.Derive.field label spec rest`: the fields `rest`, then this one. */
export const codecField = (label: string, spec: FieldSpec, rest: BlockFields): BlockFields =>
  Object.freeze([...rest, [label, spec] as const]);

/** `Codec.Derive.record name schema fields`: the codec of a `codec` block. */
export const codecRecord = (name: string, _schema: AnySchema, fields: BlockFields): AnySchema =>
  codecBlock(name, fields);

/** A refined type's `parse`: its check as a `Result`. */
export const refinedParse =
  <A>(name: string, check: (value: A) => boolean) =>
  (value: A): Result.Result<A, SchemaError> =>
    check(value) ? ok(value) : err(schemaError("", `not a valid ${name}`));

/** A refined type's witness from its base's. */
export const refinedSchema = schemaRefined;

/** `Codec.json_schema root codec`. */
export const codecJsonSchema = (root: string, codec: AnySchema): Result.Result<string, ExportError> =>
  exportJsonSchema(root, codec);

/** A Standard Schema V1 issue. */
interface StandardIssue {
  readonly message: string;
  readonly path?: ReadonlyArray<string>;
}

/** The outcome of a Standard Schema V1 validation. */
type StandardResult = { readonly value: unknown } | { readonly issues: ReadonlyArray<StandardIssue> };

/** A Standard Schema V1 object. */
export interface StandardSchema {
  readonly "~standard": {
    readonly version: 1;
    readonly vendor: "flick";
    readonly validate: (value: unknown) => StandardResult | Promise<StandardResult>;
  };
}

const issues = (cause: Cause.Cause<SchemaError>): ReadonlyArray<StandardIssue> => {
  const failure = cause.reasons.find(Cause.isFailReason);
  return failure && isSchemaError(failure.error)
    ? failure.error._0.map((i) => ({ message: i.message, path: i.path === "(root)" ? [] : i.path.split(".") }))
    : [{ message: "the codec failed" }];
};

const standardOf = (exit: Exit.Exit<unknown, SchemaError>): StandardResult =>
  Exit.isSuccess(exit) ? { value: exit.value } : { issues: issues(exit.cause) };

/** `Codec.standard`: a Standard Schema V1 validating synchronously. */
export const codecStandard = (codec: AnySchema): StandardSchema => {
  if (!isPure(codec)) return notPure();
  return Object.freeze({
    "~standard": Object.freeze({
      version: 1 as const,
      vendor: "flick" as const,
      validate: (value: unknown) => standardOf(decodePure(codec, value)),
    }),
  });
};

/**
 * `Codec.standard_async`: a Standard Schema V1 whose `validate` returns a
 * promise run with the services and under the owner of where it was made;
 * once that owner has closed, validation reports an issue instead.
 */
export const codecStandardAsync = (codec: AnySchema): Effect.Effect<StandardSchema, never, unknown> =>
  Effect.gen(function* () {
    const owner = yield* Effect.service(CurrentOwner);
    const services: Context.Context<unknown> = yield* Effect.context<unknown>();
    return Object.freeze({
      "~standard": Object.freeze({
        version: 1 as const,
        vendor: "flick" as const,
        validate: (value: unknown): Promise<StandardResult> => {
          if (owner && !owner.open) {
            return Promise.resolve({ issues: [{ message: "the scope of this validator has closed" }] });
          }
          return Effect.runPromiseExit(Effect.provideContext(codecDecode(codec, value), services)).then(standardOf);
        },
      }),
    });
  });

export const codecTransformPure = <W, A>(decodeFn: (wire: W) => A, encodeFn: (value: A) => W, from: AnySchema): AnySchema =>
  transformPure(erase(decodeFn), erase(encodeFn), from);
export const codecTransformAs = <W, A>(
  type: AnySchema,
  decodeFn: (wire: W) => A,
  encodeFn: (value: A) => W,
  from: AnySchema,
): AnySchema => transformAs(type, erase(decodeFn), erase(encodeFn), from);
export const codecTransformResult = <W, A>(
  decodeFn: (wire: W) => Result.Result<A, SchemaError>,
  encodeFn: (value: A) => W,
  from: AnySchema,
): AnySchema => transformResult(erase(decodeFn), erase(encodeFn), from);
export const codecTransformEffect = <W, A>(
  decodeFn: (wire: W) => Effect.Effect<A, SchemaError, unknown>,
  encodeFn: (value: A) => W,
  from: AnySchema,
): AnySchema => transformEffect(erase(decodeFn), erase(encodeFn), from);
export const codecCheckWhole = (predicate: (value: never) => boolean, message: string, from: AnySchema): AnySchema =>
  codecCheck(erase(predicate), message, from);

const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

/** `Checks.email`; the compiler applies the same rule to constants. */
export const checkEmail = (s: string): boolean => EMAIL.test(s);
export const checkNonEmpty = (s: string): boolean => s.length > 0;
export const checkPositive = (n: number): boolean => n > 0;

/** `Json.parse`: inert data, or a `SchemaError` with the parser's reason. */
export const jsonParse = (text: string): Result.Result<unknown, SchemaError> => {
  try {
    return ok(JSON.parse(text) as unknown);
  } catch (error) {
    return err(schemaError("", `not JSON: ${error instanceof Error ? error.message : String(error)}`));
  }
};

export const jsonStringify = (value: unknown): string | undefined => JSON.stringify(value);
