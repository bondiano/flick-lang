// Schema witnesses and JSON codecs retain their distinct wire contracts at crossings.
import { Cause as ECause, Effect, Exit as EExit, Option as EOption, Result as EResult, Schema, SchemaGetter } from "effect";
import { type Run } from "@flick-lang/runtime/rt/fiber";
import { raise } from "@flick-lang/runtime/rt/cause";
import { err, ok } from "@flick-lang/runtime/core/data";
import { mapped } from "@flick-lang/runtime/rt/effects";
import { type Witness, schemaDescription, witness } from "@flick-lang/runtime/rt/schema";
import { registerRuntimeCodec } from "@flick-lang/runtime/rt/codec";
import { schemaExportHooks } from "@flick-lang/runtime/rt/schema_export";
import { issueOf, codecFields, type Field } from "./schema/codecs.ts";
import { type SchemaError as HostSchemaError, isSchemaError, schemaError as hostSchemaError, toSchemaError } from "./schema/issues.ts";
import { codecDecode as hostCodecDecode, codecEncode as hostCodecEncode, codecJsonSchema as hostCodecJsonSchema, isPure, schemaJsonCodec as hostJsonCodec } from "./schema/index.ts";
import { type AnySchema, type Variant, schemaBigint as hostBigint, schemaInt as hostInt, schemaNumber as hostNumber, schemaString as hostString, schemaBool as hostBool, schemaUnit as hostUnit, schemaNever as hostNever, schemaUnknown as hostUnknown, schemaList, schemaRecord, schemaTuple, schemaEnum, schemaAdt, schemaLazy, schemaOption, schemaRefined, schemaResult, withJson } from "./schema/schemas.ts";
import { flickChunk, flickDuration } from "./effect/Schema.ts";
import { callEffect, fromEffect, causeFromEffect } from "./bridge_runtime.ts";
import { dataFromEffect, dataToEffect } from "./bridge_data.ts";

const isNativeCodec = (value: unknown): value is Witness =>
  typeof value === "object" && value !== null && typeof Reflect.get(value, "decode") === "function" &&
  typeof Reflect.get(value, "encode") === "function" && !Reflect.has(value, "ast");

const effectSchemas = /*#__PURE__*/ new WeakMap<Witness, unknown>();
const effectCodecs = /*#__PURE__*/ new WeakMap<Witness, unknown>();
const structuralSchemas = /*#__PURE__*/ new WeakMap<Witness, AnySchema>();
const nativeSchemas = /*#__PURE__*/ new WeakMap<object, Witness>();
/** Structural native witnesses keep their wire shape at an Effect API. */
const describedToEffect = (codec: Witness): AnySchema | undefined => {
  const original = effectSchemas.get(codec);
  if (Schema.isSchema(original)) return original;
  const description = schemaDescription(codec);
  if (description === undefined) return undefined;
  switch (description.kind) {
    case "string": return hostString;
    case "number": return hostNumber;
    case "int": return hostInt;
    case "bigint": return hostBigint;
    case "bool": return hostBool;
    case "unit": return hostUnit;
    case "never": return hostNever;
    case "unknown": return hostUnknown;
    case "duration-millis": return flickDuration;
    case "literals": return schemaEnum(description.values, null);
    case "enum-tags": return schemaEnum(description.pairs.map(([, value]) => value), description.pairs.map(([name]) => name));
    case "lazy": return schemaLazy(description.name, () => {
      const child = schemaToEffect(description.get());
      if (!Schema.isSchema(child)) throw new TypeError("a native lazy witness did not produce a schema");
      return child;
    });
    case "refined": {
      const base = describedToEffect(description.base);
      return base === undefined ? undefined : schemaRefined(
        description.name,
        base,
        (value: unknown) => codec.value(value, "")._tag === "Success",
        description.known ?? null,
      );
    }
    case "list": {
      const item = describedToEffect(description.item);
      return item === undefined ? undefined : schemaList(item);
    }
    case "chunk": {
      const item = describedToEffect(description.item);
      return item === undefined ? undefined : flickChunk(item);
    }
    case "option": {
      const item = describedToEffect(description.item);
      return item === undefined ? undefined : schemaOption(item);
    }
    case "result": {
      const error = describedToEffect(description.error);
      const value = describedToEffect(description.value);
      return error === undefined || value === undefined ? undefined : schemaResult(error, value);
    }
    case "tuple": {
      const items: Array<AnySchema> = [];
      for (const child of description.items) {
        const item = describedToEffect(child);
        if (item === undefined) return undefined;
        items.push(item);
      }
      return schemaTuple(items);
    }
    case "record": {
      const fields: Array<readonly [string, AnySchema]> = [];
      for (const [name, child] of description.fields) {
        const item = describedToEffect(child);
        if (item === undefined) return undefined;
        fields.push([name, item]);
      }
      return schemaRecord(fields);
    }
    case "adt": {
      const variants: Array<Variant> = [];
      for (const variant of description.cases) {
        const fields: Array<AnySchema> = [];
        for (const child of variant.fields) {
          const field = describedToEffect(child);
          if (field === undefined) return undefined;
          fields.push(field);
        }
        variants.push([variant.tag, variant.tag.split("#")[0] ?? variant.tag, fields]);
      }
      return schemaAdt(codec.kind, variants);
    }
    case "block": {
      const fields: Field[] = [];
      for (const [label, field] of description.fields) {
        const item = codecToEffect(field.codec);
        if (!Schema.isSchema(item)) return undefined;
        const missing = field.onMissing;
        const effectful = field.onMissingEffect;
        fields.push({
          label, wire: field.wire, codec: item, optional: field.optional,
          ...(missing === undefined ? {} : { onMissing: () => dataToEffect(missing(undefined)) }),
          ...(effectful === undefined ? {} : { onMissingEffect: () => Effect.mapError(
            callEffect(() => mapped(effectful(undefined), (value) => dataToEffect(value))),
            (error) => { if (!isSchemaError(error)) throw error; return error; },
          ) }),
        });
      }
      const made = codecFields(codec.kind.replace(/^block:/, ""), fields, description.ignoreExtra);
      return withJson(Schema.toType(made), () => made);
    }
    default: return undefined;
  }
};

/** A native codec at an Effect API boundary, with both wire directions preserved. */
export const schemaToEffect = (value: unknown): unknown => {
  if (!isNativeCodec(value)) return value;
  const codec = value;
  const original = effectSchemas.get(codec) ?? structuralSchemas.get(codec);
  if (original !== undefined) return original;
  const structural = describedToEffect(codec);
  if (structural !== undefined) {
    structuralSchemas.set(codec, structural);
    nativeSchemas.set(structural, codec);
    return structural;
  }
  const runtime = schemaDescription(codec)?.kind === "transform"
    ? Schema.Unknown
    : Schema.declare((input: unknown): input is unknown => codec.value(dataFromEffect(input), "")._tag === "Success", { expected: codec.kind });
  const fallback = Schema.Unknown.pipe(Schema.decodeTo(runtime, {
    decode: SchemaGetter.transformEffect((input: unknown) => {
      const result = codec.decode(input);
      return result._tag === "Success" ? Effect.succeed(dataToEffect(result.success)) : Effect.fail(issueOf(result.failure));
    }),
    encode: SchemaGetter.transformEffect((input: unknown) => {
      const result = codec.encode(dataFromEffect(input));
      return result._tag === "Success" ? Effect.succeed(result.success) : Effect.fail(issueOf(result.failure));
    }),
  }));
  nativeSchemas.set(runtime, codec);
  nativeSchemas.set(fallback, codec);
  return withJson(runtime, () => fallback);
};

/** A codec crosses with its wire decoder, a schema with its value witness. */
export const codecToEffect = (value: unknown): unknown => {
  if (!isNativeCodec(value)) return value;
  const original = effectCodecs.get(value);
  if (original !== undefined) return original;
  const schema = schemaToEffect(value);
  if (!Schema.isSchema(schema)) return schema;
  const codec = hostJsonCodec(schema);
  nativeSchemas.set(codec, value);
  return codec;
};

/** Effect's generated records as the plain data ABI of a native schema. */
const nativeArbitraryData = (value: unknown): unknown => {
  if (Array.isArray(value)) return value.map(nativeArbitraryData);
  if (typeof value !== "object" || value === null) return value;
  const proto: unknown = Object.getPrototypeOf(value);
  if (proto !== null && proto !== Object.prototype) return value;
  return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, nativeArbitraryData(item)]));
};

export const nativeArbitraryValue = <A>(schema: object, value: A): A =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the native schema fixes the value's ABI, not its type
  nativeSchemas.has(schema) ? nativeArbitraryData(value) as A : value;

const prefixedSchemaError = (failure: HostSchemaError, path: string): HostSchemaError => path === "" ? failure : {
  ...failure,
  _0: failure._0.map((issue) => ({ ...issue, path: issue.path === "(root)" ? path : `${path}.${issue.path}` })),
};

/** An Effect schema at the native codec boundary. */
const witnessFromEffect = (value: unknown, codec: boolean): unknown => {
  if (!Schema.isSchema(value)) return value;
  const original = nativeSchemas.get(value);
  if (original !== undefined) return original;
  const schema = codec ? value : hostJsonCodec(value);
  if (!isPure(schema)) {
    const result = (outcome: EExit.Exit<unknown, unknown>, path: string): unknown => {
      if (EExit.isSuccess(outcome)) return ok(outcome.value);
      const failure = ECause.findErrorOption(outcome.cause);
      if (EOption.isSome(failure) && isSchemaError(failure.value)) return err(prefixedSchemaError(failure.value, path));
      return raise(causeFromEffect(outcome.cause));
    };
    const native = witness("effect", () => {
      throw new TypeError("a codec with runtime decoding is no PureCodec");
    }, () => {
      throw new TypeError("a codec with runtime encoding is no PureCodec");
    });
    const bridged = registerRuntimeCodec(native, {
      decode: (input, path) => (function* (): Run {
        const outcome: unknown = yield fromEffect(Effect.exit(hostCodecDecode(schema, input)));
        if (!EExit.isExit(outcome)) throw new TypeError("an Effect codec did not return an exit");
        return result(outcome, path);
      })(),
      encode: (input, path) => (function* (): Run {
        const outcome: unknown = yield fromEffect(Effect.exit(hostCodecEncode(schema, input)));
        if (!EExit.isExit(outcome)) throw new TypeError("an Effect codec did not return an exit");
        return result(outcome, path);
      })(),
    });
    effectSchemas.set(bridged, value);
    if (codec) effectCodecs.set(bridged, value);
    return bridged;
  }
  const options = { onExcessProperty: "error", reportInput: true } as const;
  const decode = (input: unknown) => {
    const exit = Schema.decodeUnknownExit(schema)(input, options);
    if (EExit.isSuccess(exit)) return ok(exit.value);
    const failure = ECause.findErrorOption(exit.cause);
    return err(EOption.isSome(failure) ? toSchemaError(false)(failure.value) : hostSchemaError("", "Effect schema failed"));
  };
  const encode = (input: unknown) => {
    const exit = Schema.encodeUnknownExit(schema)(input, options);
    if (EExit.isSuccess(exit)) return ok(exit.value);
    const failure = ECause.findErrorOption(exit.cause);
    return err(EOption.isSome(failure) ? toSchemaError(true)(failure.value) : hostSchemaError("", "Effect schema failed"));
  };
  const native = witness("effect", (input) => decode(input), (input) => encode(input));
  effectSchemas.set(native, value);
  if (codec) effectCodecs.set(native, value);
  return native;
};
export const schemaFromEffect = (value: unknown): unknown => witnessFromEffect(value, false);
export const codecFromEffect = (value: unknown): unknown => witnessFromEffect(value, true);


export const installSchemaBridge = (): void => {
schemaExportHooks.host = (name, codec) => {
  if (!isNativeCodec(codec)) return undefined;
  const sourceCodec = codecToEffect(codec);
  if (!Schema.isSchema(sourceCodec)) return undefined;
  const exported = hostCodecJsonSchema(name, sourceCodec);
  return EResult.isSuccess(exported) ? ok(exported.success) : err(exported.failure);
};

};
