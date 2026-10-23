// The source API of schemas and codecs over Effect Schema: decoding and
// encoding with the program's `SchemaError`, the Standard Schema adapter,
// the `codec` DSL and refined types.
import { Cause, Effect, Exit, Schema } from "effect";

import { err, ok } from "../data.js";
import { CurrentOwner } from "../owner.js";
import { registerCodec } from "./aot.js";
import { codecCheck, codecFields, codecTransform } from "./codecs.js";
import { exportJsonSchema } from "./exporter.js";
import { requireInert, schemaError, toSchemaError } from "./issues.js";
import { jsonOf, schemaRefined } from "./schemas.js";

export * from "./codecs.js";
export * from "./schemas.js";
export { enableSchemaAot, installSchemaAot, schemaCompilerRuntime, writeSchemaAot } from "./aot.js";
export { schemaError };

/** Wire input is strict: a key no field names is an error. */
const OPTIONS = { onExcessProperty: "error", reportInput: true };

/** Whether decoding with `ast` may need services or wait: a runtime transformation or default. */
const runtimeAsts = new WeakMap();
const reaches = (ast, seen) => {
  const notes = ast.annotations ?? {};
  // a generic recursive type makes a new node per level: its name stands for all of them
  const key = notes["flick/ref"] ?? ast;
  if (seen.has(key)) return false;
  seen.add(key);
  if (notes["flick/transform"] === "effect" || notes["flick/effect"] === true) return true;
  const children = [
    ...(ast.encoding ?? []).map((link) => link.to),
    ...(ast.propertySignatures ?? []).map((p) => p.type),
    ...(ast.indexSignatures ?? []).map((i) => i.type),
    ...(ast.elements ?? []),
    ...(ast.rest ?? []),
    ...(ast.types ?? []),
    ...(ast.typeParameters ?? []),
    ...(ast._tag === "Suspend" ? [ast.thunk()] : []),
  ];
  return children.some((child) => reaches(child, seen));
};
const needsRuntime = (ast) => {
  if (!runtimeAsts.has(ast)) runtimeAsts.set(ast, reaches(ast, new Set()));
  return runtimeAsts.get(ast);
};

const fromExit = (exit, encoding) =>
  Exit.isSuccess(exit) ? Effect.succeed(exit.value) : Effect.failCause(Cause.map(exit.cause, (e) => toSchemaError(e, encoding)));

/** `Codec.decode codec input`: an effect failing with `SchemaError`. */
export const codecDecode = (codec, input) =>
  Effect.suspend(() => {
    const problem = requireInert(input);
    if (problem) return problem;
    if (needsRuntime(codec.ast)) {
      return Effect.mapError(Schema.decodeUnknownEffect(codec)(input, OPTIONS), (e) => toSchemaError(e, false));
    }
    // a pure codec decodes synchronously, through compiled decoders when installed
    return fromExit(Schema.decodeUnknownExit(codec)(input, OPTIONS), false);
  });

/** `Codec.encode codec value`. */
export const codecEncode = (codec, value) =>
  Effect.suspend(() =>
    needsRuntime(codec.ast)
      ? Effect.mapError(Schema.encodeUnknownEffect(codec)(value, OPTIONS), (e) => toSchemaError(e, true))
      : fromExit(Schema.encodeUnknownExit(codec)(value, OPTIONS), true),
  );

const result = (effect) => {
  const exit = Effect.runSyncExit(effect);
  if (Exit.isSuccess(exit)) return ok(exit.value);
  const failure = exit.cause.reasons.find(Cause.isFailReason);
  if (failure) return err(failure.error);
  throw Cause.squash(exit.cause);
};

/** `Codec.decode_result`: a pure codec runs synchronously. */
export const codecDecodeResult = (codec, input) => result(codecDecode(codec, input));
export const codecEncodeResult = (codec, value) => result(codecEncode(codec, value));

/** `Schema.validate schema input`: whether `input` is a value of the type. */
export const schemaValidate = (schema, input) => result(fromExit(Schema.decodeUnknownExit(schema)(input, OPTIONS), false));

/** `Codec.json schema`: the canonical JSON codec, compiled when installed. */
export const schemaJsonCodec = (schema) => registerCodec(jsonOf(schema));

/** A field of a `codec` block, before its label is known. */
export const dslField = (wire, codec) => Object.freeze({ wire, codec, optional: false });
export const dslOptionalField = (wire, codec) => Object.freeze({ wire, codec, optional: true });
export const dslFieldEffect = (wire, codec) => Object.freeze({ wire, codec, optional: false });
export const dslOnMissing = (factory, field) => Object.freeze({ ...field, onMissing: factory });
export const dslOnMissingEffect = (factory, field) =>
  Object.freeze({ ...field, onMissingEffect: factory, codec: field.codec.annotate({ "flick/effect": true }) });

/**
 * The codec of a `codec` block: `fields` lists `[label, field]`. Two fields
 * sharing a wire name is a defect, when the compiler could not see it.
 */
export const codecBlock = (name, fields) => {
  const wires = new Set();
  for (const [, f] of fields) {
    if (wires.has(f.wire)) throw new Error(`two fields of the codec of ${name} use the wire name "${f.wire}"`);
    wires.add(f.wire);
  }
  return registerCodec(codecFields(name, fields.map(([label, f]) => Object.freeze({ label, ...f })), false));
};

/** A refined type's `parse`: its check as a `Result`. */
export const refinedParse = (name, check) => (value) =>
  check(value) ? ok(value) : err(schemaError("", `not a valid ${name}`));

/** A refined type's witness from its base's. */
export const refinedSchema = (name, base, check, known) => schemaRefined(name, base, check, known);

/** `Codec.json_schema root codec`. */
export const codecJsonSchema = (root, codec) => {
  const out = exportJsonSchema(root, codec);
  return out._tag === "Success" ? ok(out.value) : err(out.error);
};

const issues = (cause) => {
  const failure = cause.reasons.find(Cause.isFailReason);
  return failure
    ? failure.error._0.map((i) => ({ message: i.message, path: i.path === "(root)" ? [] : i.path.split(".") }))
    : [{ message: "the codec failed" }];
};

/** `Codec.standard`: a Standard Schema V1 validating synchronously. */
export const codecStandard = (codec) =>
  Object.freeze({
    "~standard": Object.freeze({
      version: 1,
      vendor: "flick",
      validate: (value) => {
        const exit = Effect.runSyncExit(codecDecode(codec, value));
        return Exit.isSuccess(exit) ? { value: exit.value } : { issues: issues(exit.cause) };
      },
    }),
  });

/**
 * `Codec.standard_async`: a Standard Schema V1 whose `validate` returns a
 * promise run with the services and under the owner of where it was made;
 * once that owner has closed, validation reports an issue instead.
 */
export const codecStandardAsync = (codec) =>
  Effect.gen(function* () {
    const owner = yield* Effect.service(CurrentOwner);
    const services = yield* Effect.context();
    return Object.freeze({
      "~standard": Object.freeze({
        version: 1,
        vendor: "flick",
        validate: (value) => {
          if (owner && !owner.open) {
            return Promise.resolve({ issues: [{ message: "the scope of this validator has closed" }] });
          }
          return Effect.runPromiseExit(Effect.provideContext(codecDecode(codec, value), services)).then((exit) =>
            Exit.isSuccess(exit) ? { value: exit.value } : { issues: issues(exit.cause) },
          );
        },
      }),
    });
  });

export const codecTransformPure = (decodeFn, encodeFn, from) => codecTransform("pure", decodeFn, encodeFn, from);
export const codecTransformResult = (decodeFn, encodeFn, from) => codecTransform("result", decodeFn, encodeFn, from);
export const codecTransformEffect = (decodeFn, encodeFn, from) => codecTransform("effect", decodeFn, encodeFn, from);
export const codecCheckWhole = (predicate, message, from) => codecCheck(predicate, message, from);

const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

/** `Checks.email`; the compiler applies the same rule to constants. */
export const checkEmail = (s) => EMAIL.test(s);
export const checkNonEmpty = (s) => s.length > 0;
export const checkPositive = (n) => n > 0;

/** `Json.parse`: inert data, or a `SchemaError` with the parser's reason. */
export const jsonParse = (text) => {
  try {
    return ok(JSON.parse(text));
  } catch (error) {
    return err(schemaError("", `not JSON: ${error.message}`));
  }
};

export const jsonStringify = (value) => JSON.stringify(value);
