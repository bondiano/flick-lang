// Ahead-of-use compilation of pure codecs (`schema-aot`): each node becomes
// a closure built once, instead of a walk of the tree on every decode. A
// node it does not compile keeps the interpreter for its subtree, and an
// effectful codec stays wholly on the interpreter, so the outcome of every
// decode is the same either way.
import { Effect, Exit, Option } from "effect";

import { context, decode, schemaError } from "./interpret.js";

class Issue extends Error {
  constructor(path, message) {
    super(message);
    this.path = path;
  }
}

const at = (path, key) => (typeof key === "number" ? `${path}[${key}]` : path ? `${path}.${key}` : key);

const own = (object, key) => {
  const d = Object.getOwnPropertyDescriptor(object, key);
  return d && "value" in d ? { found: true, value: d.value } : { found: false };
};

const isObject = (v) => typeof v === "object" && v !== null && !Array.isArray(v);

const describe = (input) => {
  if (input === null) return "null";
  if (typeof input === "number" && !Number.isFinite(input)) return String(input);
  if (Array.isArray(input)) return "an array";
  return typeof input === "object" ? "an object" : typeof input;
};

/** Runs the interpreter on a subtree, as a compiled closure. */
const interpreted = (codec) => (input, path, ctx) => {
  const exit = Effect.runSyncExit(decode(codec, input, path, ctx));
  if (Exit.isSuccess(exit)) return exit.value;
  const failure = exit.cause.reasons.find((r) => r._tag === "Fail");
  if (failure) {
    const [issue] = failure.error._0;
    throw new Issue(issue.path === "(root)" ? "" : issue.path, issue.message);
  }
  throw exit.cause;
};

const guarded = (body) => (input, path, ctx) => {
  if (typeof input !== "object" || input === null) return body(input, path, ctx);
  if (ctx.active.has(input)) throw new Issue(path, "a cycle in the input");
  ctx.active.add(input);
  try {
    return body(input, path, ctx);
  } finally {
    ctx.active.delete(input);
  }
};

/** Whether decoding with `codec` may run anything but pure code. */
const effectful = (codec) =>
  (codec.kind === "transform" && (codec.mode === "effect" || effectful(codec.from))) ||
  (codec.kind === "fields" && codec.fields.some((f) => f.onMissingEffect || effectful(f.codec))) ||
  (codec.item !== undefined && typeof codec.item === "object" && codec.item.kind && effectful(codec.item)) ||
  (codec.kind === "check" && effectful(codec.from));

const compileNode = (codec) => {
  switch (codec.kind) {
    case "jsonString":
      return (input, path) => {
        if (typeof input !== "string") throw new Issue(path, `expected a string, got ${describe(input)}`);
        return input;
      };
    case "jsonNumberFinite":
      return (input, path) => {
        if (typeof input !== "number" || !Number.isFinite(input)) {
          throw new Issue(path, `expected a finite number, got ${describe(input)}`);
        }
        return input;
      };
    case "jsonBool":
      return (input, path) => {
        if (typeof input !== "boolean") throw new Issue(path, `expected a boolean, got ${describe(input)}`);
        return input;
      };
    case "jsonList": {
      const item = compileNode(codec.item);
      return guarded((input, path, ctx) => {
        if (!Array.isArray(input)) throw new Issue(path, `expected an array, got ${describe(input)}`);
        return input.map((x, i) => item(x, at(path, i), ctx));
      });
    }
    case "optionNull": {
      const item = compileNode(codec.item);
      return (input, path, ctx) => {
        if (input === undefined) throw new Issue(path, "undefined is not JSON: expected null or a value");
        return input === null ? Option.none() : Option.some(item(input, path, ctx));
      };
    }
    case "optionNullish": {
      const item = compileNode(codec.item);
      return (input, path, ctx) =>
        input === null || input === undefined ? Option.none() : Option.some(item(input, path, ctx));
    }
    case "fields": {
      const fields = codec.fields.map((f) => ({ ...f, run: compileNode(f.codec) }));
      const wires = new Set(codec.fields.map((f) => f.wire));
      return guarded((input, path, ctx) => {
        if (!isObject(input)) throw new Issue(path, `expected an object, got ${describe(input)}`);
        if (!codec.ignoreExtra) {
          const extra = Object.keys(input).find((k) => !wires.has(k));
          if (extra !== undefined) throw new Issue(at(path, extra), "an unknown key");
        }
        const out = {};
        for (const f of fields) {
          const p = at(path, f.wire);
          const v = own(input, f.wire);
          if (!v.found || (f.optional && v.value === undefined)) {
            if (f.optional || f.codec.kind === "optionNullish") out[f.label] = Option.none();
            else if (f.onMissing) out[f.label] = f.onMissing(undefined);
            else throw new Issue(p, "a missing key");
            continue;
          }
          const x = f.run(v.value, p, ctx);
          out[f.label] = f.optional ? Option.some(x) : x;
        }
        return Object.freeze(out);
      });
    }
    case "transform": {
      const from = compileNode(codec.from);
      if (codec.mode === "pure") return (input, path, ctx) => codec.decode(from(input, path, ctx));
      return interpreted(codec);
    }
    case "check": {
      const from = compileNode(codec.from);
      return (input, path, ctx) => {
        const v = from(input, path, ctx);
        if (!codec.check(v)) throw new Issue(path, codec.message);
        return v;
      };
    }
    default:
      return interpreted(codec);
  }
};

/** A compiled decoder of `codec`, or `undefined` for an effectful codec. */
export const compileDecoder = (codec) => {
  if (effectful(codec)) return undefined;
  const run = compileNode(codec);
  return (input) =>
    Effect.suspend(() => {
      try {
        return Effect.succeed(run(input, "", context()));
      } catch (error) {
        if (error instanceof Issue) return Effect.fail(schemaError(error.path, error.message));
        throw error;
      }
    });
};
