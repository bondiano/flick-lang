// The source API of schemas and codecs over the interpreter, the exporter,
// the Standard Schema adapter, generators and the compiled decoders.
import { Cause, Effect, Exit, Option } from "effect";

import { err, ok } from "../data.js";
import { CurrentOwner } from "../owner.js";
import { compileDecoder } from "./compile.js";
import { exportJsonSchema } from "./exporter.js";
import { check, context, decode, encode, schemaError } from "./interpret.js";
import {
  codecCheck as codecCheckNode,
  codecFields,
  codecJson,
  codecTransform as codecTransformNode,
  schemaRefined,
} from "./nodes.js";

export * from "./nodes.js";

/** Compiled decoders, when the entry point turned them on (`schema-aot`). */
let compiled;

/** Turns on compiled decoding of pure codecs for the run. */
export const enableSchemaAot = () => {
  compiled ??= new WeakMap();
};

const decoder = (codec) => {
  if (compiled === undefined) return undefined;
  if (!compiled.has(codec)) compiled.set(codec, compileDecoder(codec));
  return compiled.get(codec);
};

/** `Codec.decode codec input`: an effect failing with `SchemaError`. */
export const codecDecode = (codec, input) => decoder(codec)?.(input) ?? decode(codec, input, "", context());

/** `Codec.encode codec value`. */
export const codecEncode = (codec, value) => encode(codec, value, "");

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
export const schemaValidate = (schema, input) =>
  result(Effect.as(check(schema, input, "", context()), input));

export const schemaJsonCodec = (schema) => codecJson(schema);

/** A field of a `codec` block, before its label is known. */
export const dslField = (wire, codec) => Object.freeze({ wire, codec, optional: false });
export const dslOptionalField = (wire, codec) => Object.freeze({ wire, codec, optional: true });
export const dslFieldEffect = (wire, codec) => Object.freeze({ wire, codec, optional: false });
export const dslOnMissing = (factory, field) => Object.freeze({ ...field, onMissing: factory });
export const dslOnMissingEffect = (factory, field) => Object.freeze({ ...field, onMissingEffect: factory });

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
  return codecFields(name, fields.map(([label, f]) => Object.freeze({ label, ...f })), false);
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

/** `Option` of a decoded value, for tests of the interpreter. */
export const decodedOption = (o) => (Option.isSome(o) ? o.value : undefined);

export const codecTransformPure = (decodeFn, encodeFn, from) =>
  codecTransformNode("pure", decodeFn, encodeFn, from);
export const codecTransformResult = (decodeFn, encodeFn, from) =>
  codecTransformNode("result", decodeFn, encodeFn, from);
export const codecTransformEffect = (decodeFn, encodeFn, from) =>
  codecTransformNode("effect", decodeFn, encodeFn, from);
export const codecCheckWhole = (predicate, message, from) => codecCheckNode(predicate, message, from);

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
