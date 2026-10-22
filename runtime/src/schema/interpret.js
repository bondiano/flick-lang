// The interpreter of schemas and codecs: the baseline every faster path
// must agree with. Decoding reads only own data properties of its input,
// fails with a `SchemaError` listing `{ path, message }` issues, and treats
// a value met again on the path it is decoding as a cycle; a value shared
// by two paths is fine.
import { Effect, Option, Redacted, Result } from "effect";

const SCHEMA_ERROR = "Prelude.SchemaError.SchemaError";

/** A `SchemaError` of one issue. */
export const schemaError = (path, message) => ({
  _tag: SCHEMA_ERROR,
  _0: [Object.freeze({ path: path || "(root)", message })],
});

const fail = (path, message) => Effect.fail(schemaError(path, message));

const at = (path, key) => (typeof key === "number" ? `${path}[${key}]` : path ? `${path}.${key}` : key);

const describe = (input) => {
  if (input === null) return "null";
  if (typeof input === "number" && !Number.isFinite(input)) return String(input);
  if (Array.isArray(input)) return "an array";
  return typeof input === "object" ? "an object" : typeof input;
};

/** An own data property, never a getter. */
const own = (object, key) => {
  const d = Object.getOwnPropertyDescriptor(object, key);
  return d && "value" in d ? { found: true, value: d.value } : { found: false };
};

const isObject = (v) => typeof v === "object" && v !== null && !Array.isArray(v);

/** Runs `body` with `input` on the active path, refusing a cycle. */
const entering = (ctx, input, path, body) =>
  Effect.suspend(() => {
    if (typeof input !== "object" || input === null) return body;
    if (ctx.active.has(input)) return fail(path, "a cycle in the input");
    ctx.active.add(input);
    return Effect.ensuring(body, Effect.sync(() => ctx.active.delete(input)));
  });

const unlazy = (node) => (node.kind === "lazy" ? unlazy(node.get()) : node);

// ---- schemas: a value of the program -------------------------------------

/** Checks that `value` is a value of `schema`, as the program represents it. */
export const check = (schema, value, path, ctx) =>
  Effect.suspend(() => {
    const s = unlazy(schema);
    switch (s.kind) {
      case "number":
        return typeof value === "number" ? Effect.void : fail(path, `expected a number, got ${describe(value)}`);
      case "string":
        return typeof value === "string" ? Effect.void : fail(path, `expected a string, got ${describe(value)}`);
      case "bool":
        return typeof value === "boolean" ? Effect.void : fail(path, `expected a boolean, got ${describe(value)}`);
      case "bigint":
        return typeof value === "bigint" ? Effect.void : fail(path, `expected a BigInt, got ${describe(value)}`);
      case "unit":
        return value === undefined ? Effect.void : fail(path, "expected ()");
      case "never":
        return fail(path, "no value has type Never");
      case "unknown":
      case "opaque":
        return Effect.void;
      case "list":
        if (!Array.isArray(value)) return fail(path, `expected a list, got ${describe(value)}`);
        return entering(ctx, value, path, Effect.forEach(value, (x, i) => check(s.item, x, at(path, i), ctx), { discard: true }));
      case "tuple":
        if (!Array.isArray(value) || value.length !== s.items.length) {
          return fail(path, `expected a tuple of ${s.items.length}`);
        }
        return Effect.forEach(s.items, (item, i) => check(item, value[i], at(path, i), ctx), { discard: true });
      case "option":
        if (!Option.isOption(value)) return fail(path, "expected an Option");
        return Option.isSome(value) ? check(s.item, value.value, path, ctx) : Effect.void;
      case "result":
        if (!Result.isResult(value)) return fail(path, "expected a Result");
        return Result.isSuccess(value)
          ? check(s.value, value.success, path, ctx)
          : check(s.error, value.failure, path, ctx);
      case "redacted":
        return Redacted.isRedacted(value) ? Effect.void : fail(path, "expected a Redacted value");
      case "record":
        if (!isObject(value)) return fail(path, `expected a record, got ${describe(value)}`);
        return entering(
          ctx,
          value,
          path,
          Effect.forEach(s.fields, ([label, field]) => check(field, own(value, label).value, at(path, label), ctx), {
            discard: true,
          }),
        );
      case "adt": {
        const variant = isObject(value) ? s.variants.find(([tag]) => tag === own(value, "_tag").value) : undefined;
        if (!variant) return fail(path, `expected a ${s.name}`);
        const [, name, fields] = variant;
        return Effect.forEach(fields, (f, i) => check(f, own(value, `_${i}`).value, at(at(path, name), i), ctx), {
          discard: true,
        });
      }
      case "refined":
        return Effect.flatMap(check(s.base, value, path, ctx), () =>
          s.check(value) ? Effect.void : fail(path, `not a valid ${s.name}`),
        );
      default:
        return fail(path, `unknown schema node ${s.kind}`);
    }
  });

// ---- codecs: wire to program and back -------------------------------------

const finite = (n) => typeof n === "number" && Number.isFinite(n);

/** The canonical JSON decoding of a schema. */
const decodeJson = (schema, input, path, ctx) =>
  Effect.suspend(() => {
    const s = unlazy(schema);
    switch (s.kind) {
      case "number":
        return finite(input) ? Effect.succeed(input) : fail(path, `expected a finite number, got ${describe(input)}`);
      case "string":
        return typeof input === "string" ? Effect.succeed(input) : fail(path, `expected a string, got ${describe(input)}`);
      case "bool":
        return typeof input === "boolean" ? Effect.succeed(input) : fail(path, `expected a boolean, got ${describe(input)}`);
      case "bigint":
        return typeof input === "string" && /^-?\d+$/.test(input)
          ? Effect.succeed(BigInt(input))
          : fail(path, "expected a string of digits");
      case "unit":
        return input === null ? Effect.succeed(undefined) : fail(path, "expected null");
      case "never":
        return fail(path, "no value has type Never");
      case "list":
        if (!Array.isArray(input)) return fail(path, `expected an array, got ${describe(input)}`);
        return entering(ctx, input, path, Effect.forEach(input, (x, i) => decodeJson(s.item, x, at(path, i), ctx)));
      case "tuple":
        if (!Array.isArray(input) || input.length !== s.items.length) {
          return fail(path, `expected an array of ${s.items.length}`);
        }
        return entering(ctx, input, path, Effect.forEach(s.items, (item, i) => decodeJson(item, input[i], at(path, i), ctx)));
      case "option":
        return input === null
          ? Effect.succeed(Option.none())
          : Effect.map(decodeJson(s.item, input, path, ctx), Option.some);
      case "result": {
        if (!isObject(input)) return fail(path, "expected an object with `ok` or `err`");
        const okv = own(input, "ok");
        if (okv.found) return Effect.map(decodeJson(s.value, okv.value, at(path, "ok"), ctx), Result.succeed);
        const errv = own(input, "err");
        if (errv.found) return Effect.map(decodeJson(s.error, errv.value, at(path, "err"), ctx), Result.fail);
        return fail(path, "expected an object with `ok` or `err`");
      }
      case "record":
        return decodeRecord(
          s.fields.map(([label, field]) => ({ label, wire: label, codec: { kind: "json", schema: field } })),
          false,
          input,
          path,
          ctx,
        );
      case "adt": {
        if (!isObject(input)) return fail(path, `expected an object with a \`_tag\`, got ${describe(input)}`);
        const name = own(input, "_tag").value;
        const variant = s.variants.find(([, n]) => n === name);
        if (!variant) return fail(at(path, "_tag"), `expected one of ${s.variants.map(([, n]) => n).join(", ")}`);
        const [tag, , fields] = variant;
        const allowed = new Set(["_tag", ...fields.map((_, i) => `_${i}`)]);
        const extra = Object.keys(input).find((k) => !allowed.has(k));
        if (extra !== undefined) return fail(at(path, extra), "an unknown key");
        return entering(
          ctx,
          input,
          path,
          Effect.map(
            Effect.forEach(fields, (f, i) => {
              const v = own(input, `_${i}`);
              return v.found ? decodeJson(f, v.value, at(path, `_${i}`), ctx) : fail(at(path, `_${i}`), "a missing key");
            }),
            (values) => Object.freeze(Object.fromEntries([["_tag", tag], ...values.map((v, i) => [`_${i}`, v])])),
          ),
        );
      }
      case "refined":
        return Effect.flatMap(decodeJson(s.base, input, path, ctx), (v) =>
          s.check(v) ? Effect.succeed(v) : fail(path, `not a valid ${s.name}`),
        );
      case "redacted":
        return fail(path, "a Redacted value has no JSON form without an explicit policy");
      case "opaque":
        return fail(path, s.reason);
      case "unknown":
        return Effect.succeed(input);
      default:
        return fail(path, `unknown schema node ${s.kind}`);
    }
  });

const encodeJson = (schema, value, path) =>
  Effect.suspend(() => {
    const s = unlazy(schema);
    switch (s.kind) {
      case "number":
        return finite(value) ? Effect.succeed(value) : fail(path, `${value} has no JSON form`);
      case "string":
      case "bool":
      case "unknown":
        return Effect.succeed(value);
      case "bigint":
        return Effect.succeed(String(value));
      case "unit":
        return Effect.succeed(null);
      case "list":
        return Effect.forEach(value, (x, i) => encodeJson(s.item, x, at(path, i)));
      case "tuple":
        return Effect.forEach(s.items, (item, i) => encodeJson(item, value[i], at(path, i)));
      case "option":
        return Option.isSome(value) ? encodeJson(s.item, value.value, path) : Effect.succeed(null);
      case "result":
        return Result.isSuccess(value)
          ? Effect.map(encodeJson(s.value, value.success, at(path, "ok")), (ok) => ({ ok }))
          : Effect.map(encodeJson(s.error, value.failure, at(path, "err")), (err) => ({ err }));
      case "record":
        return Effect.map(
          Effect.forEach(s.fields, ([label, field]) =>
            Effect.map(encodeJson(field, value[label], at(path, label)), (v) => [label, v]),
          ),
          Object.fromEntries,
        );
      case "adt": {
        const [, name, fields] = s.variants.find(([tag]) => tag === value._tag);
        return Effect.map(
          Effect.forEach(fields, (f, i) => encodeJson(f, value[`_${i}`], at(path, `_${i}`))),
          (values) => Object.fromEntries([["_tag", name], ...values.map((v, i) => [`_${i}`, v])]),
        );
      }
      case "refined":
        return encodeJson(s.base, value, path);
      case "redacted":
        return fail(path, "a Redacted value has no JSON form without an explicit policy");
      case "opaque":
        return fail(path, s.reason);
      default:
        return fail(path, `${s.kind} has no JSON form`);
    }
  });

/** Decodes the fields of a `codec` block, or of a canonical record. */
const decodeRecord = (fields, ignoreExtra, input, path, ctx) =>
  Effect.suspend(() => {
    if (!isObject(input)) return fail(path, `expected an object, got ${describe(input)}`);
    if (!ignoreExtra) {
      const wires = new Set(fields.map((f) => f.wire));
      const extra = Object.keys(input).find((k) => !wires.has(k));
      if (extra !== undefined) return fail(at(path, extra), "an unknown key");
    }
    return entering(
      ctx,
      input,
      path,
      Effect.map(
        Effect.forEach(fields, (f) => {
          const p = at(path, f.wire);
          const v = own(input, f.wire);
          if (!v.found || (f.optional && v.value === undefined)) {
            if (f.optional || f.codec.kind === "optionNullish") return Effect.succeed([f.label, Option.none()]);
            if (f.onMissing) return Effect.sync(() => [f.label, f.onMissing(undefined)]);
            if (f.onMissingEffect) return Effect.map(f.onMissingEffect(undefined), (x) => [f.label, x]);
            return fail(p, "a missing key");
          }
          const decoded = decode(f.codec, v.value, p, ctx);
          return Effect.map(decoded, (x) => [f.label, f.optional ? Option.some(x) : x]);
        }),
        (entries) => Object.freeze(Object.fromEntries(entries)),
      ),
    );
  });

/** Decodes `input` with `codec` into a value of the program. */
export const decode = (codec, input, path, ctx) =>
  Effect.suspend(() => {
    switch (codec.kind) {
      case "json":
        return decodeJson(codec.schema, input, path, ctx);
      case "jsonString":
        return decodeJson({ kind: "string" }, input, path, ctx);
      case "jsonNumberFinite":
        return decodeJson({ kind: "number" }, input, path, ctx);
      case "jsonBool":
        return decodeJson({ kind: "bool" }, input, path, ctx);
      case "jsonBigint":
        return decodeJson({ kind: "bigint" }, input, path, ctx);
      case "jsonNumberLossless":
        if (typeof input === "number") return Effect.succeed(input);
        if (input === "NaN" || input === "Infinity" || input === "-Infinity") return Effect.succeed(Number(input));
        if (input === "-0") return Effect.succeed(-0);
        return fail(path, `expected a number, got ${describe(input)}`);
      case "jsonList":
        if (!Array.isArray(input)) return fail(path, `expected an array, got ${describe(input)}`);
        return entering(ctx, input, path, Effect.forEach(input, (x, i) => decode(codec.item, x, at(path, i), ctx)));
      case "optionNull":
        if (input === undefined) return fail(path, "undefined is not JSON: expected null or a value");
        return input === null ? Effect.succeed(Option.none()) : Effect.map(decode(codec.item, input, path, ctx), Option.some);
      case "optionNullish":
        return input === null || input === undefined
          ? Effect.succeed(Option.none())
          : Effect.map(decode(codec.item, input, path, ctx), Option.some);
      case "taggedOption": {
        if (!isObject(input)) return fail(path, "expected an object with `_tag` None or Some");
        const tag = own(input, "_tag").value;
        if (tag === "None") return Effect.succeed(Option.none());
        if (tag !== "Some") return fail(at(path, "_tag"), "expected None or Some");
        return Effect.map(decode(codec.item, own(input, "value").value, at(path, "value"), ctx), Option.some);
      }
      case "fields":
        return decodeRecord(codec.fields, codec.ignoreExtra, input, path, ctx);
      case "transform":
        return Effect.flatMap(decode(codec.from, input, path, ctx), (v) => {
          if (codec.mode === "pure") return Effect.sync(() => codec.decode(v));
          if (codec.mode === "result") {
            const r = codec.decode(v);
            return r._tag === "Success" ? Effect.succeed(r.success) : Effect.fail(r.failure);
          }
          return codec.decode(v);
        });
      case "check":
        return Effect.flatMap(decode(codec.from, input, path, ctx), (v) =>
          codec.check(v) ? Effect.succeed(v) : fail(path, codec.message),
        );
      case "redactedReveal":
        return Effect.map(decode(codec.item, input, path, ctx), (v) => Redacted.make(v));
      case "redactedMasked":
        return fail(path, "a masked Redacted value cannot be decoded");
      default:
        return fail(path, `unknown codec node ${codec.kind}`);
    }
  });

/** Encodes a value of the program with `codec`. */
export const encode = (codec, value, path) =>
  Effect.suspend(() => {
    switch (codec.kind) {
      case "json":
        return encodeJson(codec.schema, value, path);
      case "jsonString":
      case "jsonBool":
        return Effect.succeed(value);
      case "jsonNumberFinite":
        return finite(value) ? Effect.succeed(value) : fail(path, `${value} has no JSON form`);
      case "jsonBigint":
        return Effect.succeed(String(value));
      case "jsonNumberLossless":
        return Effect.succeed(Number.isFinite(value) && !Object.is(value, -0) ? value : String(value === 0 ? "-0" : value));
      case "jsonList":
        return Effect.forEach(value, (x, i) => encode(codec.item, x, at(path, i)));
      case "optionNull":
      case "optionNullish":
        return Option.isSome(value) ? encode(codec.item, value.value, path) : Effect.succeed(null);
      case "taggedOption":
        return Option.isSome(value)
          ? Effect.map(encode(codec.item, value.value, at(path, "value")), (v) => ({ _tag: "Some", value: v }))
          : Effect.succeed({ _tag: "None" });
      case "fields":
        return Effect.map(
          Effect.forEach(codec.fields, (f) => {
            const v = value[f.label];
            if (f.optional && Option.isNone(v)) return Effect.succeed(undefined);
            const inner = f.optional ? v.value : v;
            return Effect.map(encode(f.codec, inner, at(path, f.wire)), (w) => [f.wire, w]);
          }),
          (entries) => Object.fromEntries(entries.filter((e) => e !== undefined)),
        );
      case "transform":
        return Effect.flatMap(Effect.sync(() => codec.encode(value)), (v) => encode(codec.from, v, path));
      case "check":
        return codec.check(value) ? encode(codec.from, value, path) : fail(path, codec.message);
      case "redactedReveal":
        return encode(codec.item, Redacted.value(value), path);
      case "redactedMasked":
        return Effect.succeed("<redacted>");
      default:
        return fail(path, `unknown codec node ${codec.kind}`);
    }
  });

export const context = () => ({ active: new Set() });
