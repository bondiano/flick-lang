// `core/` is all a program built with `effect = "none"` imports: it has every
// name the built-in modules of such a build bind and every helper compiled
// code calls, and none of its modules imports Effect.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { bigintDivide, bigintRemainder, intParse } from "../src/core/text.ts";
import { genConstant, testProperty } from "../src/core/testing.ts";
import { equals, hash, none, some } from "../src/core/data.ts";

test("Effect and native Options have the same equality and hash", () => {
  const hostNone: unknown = Object.create({ _tag: "None" });
  const hostSome: unknown = Object.assign(Object.create({ _tag: "Some" }), { value: 7 });
  assert.equal(equals(hostNone, none), true);
  assert.equal(equals(none, hostNone), true);
  assert.equal(hash(hostNone), hash(none));
  assert.equal(equals(hostSome, some(7)), true);
  assert.equal(equals(some(7), hostSome), true);
  assert.equal(hash(hostSome), hash(some(7)));
});

test("distinct class instances without a Flick equality hook compare by identity", () => {
  const first = new URL("https://example.com/a");
  const second = new URL("https://example.com/b");
  assert.equal(equals(first, first), true);
  assert.equal(equals(first, second), false);
  assert.equal(equals(new Date(1), new Date(2)), false);
});

test("a host value with Effect equality and hash keeps its semantic identity", () => {
  class HostValue {
    readonly value: number;
    constructor(value: number) { this.value = value; }
    ["~effect/Equal"](other: unknown): boolean {
      return other instanceof HostValue && this.value === other.value;
    }
    ["~effect/Hash"](): number {
      return this.value;
    }
  }
  assert.equal(equals(new HostValue(1), new HostValue(1)), true);
  assert.equal(equals(new HostValue(1), new HostValue(2)), false);
  assert.equal(hash(new HostValue(1)), hash(new HostValue(1)));
});

test("property failures preview native Option and Result like the other backends", () => {
  const value = [{ b: { _tag: "Some", value: { _tag: "Success", success: 1 } } }];
  const property = testProperty("values", genConstant(value), () => ({ _tag: "failed", _0: "no" }));
  assert.equal(property.kind, "property");
  if (property.kind === "property") {
    assert.equal(
      property.run(() => 0, 0),
      'no (case 0: [{"b":{"_id":"Option","_tag":"Some","value":{"_id":"Result","_tag":"Success","value":1}}}])',
    );
  }
});

test("checked BigInt division keeps a zero divisor as an arithmetic error", () => {
  assert.deepEqual(bigintDivide(7n, 2n), { _tag: "Success", success: 3n });
  assert.deepEqual(bigintRemainder(-7n, 2n), { _tag: "Success", success: -1n });
  const result = bigintDivide(5n, 0n);
  assert.equal(result._tag, "Failure");
  if (result._tag === "Failure") assert.equal(result.failure._0, "division by zero");
});

test("intParse follows ECMAScript whitespace, signed 32-bit range and normalized zero", () => {
  const accepted: ReadonlyArray<readonly [string, number]> = [
    ["36", 36],
    ["+36", 36],
    ["-36", -36],
    ["0", 0],
    ["+0", 0],
    ["-0", 0],
    ["000000000000000000000000036", 36],
    [" \t\n\v\f\r 36 \r\f\v\n\t ", 36],
    ["\uFEFF36", 36],
    ["36\uFEFF", 36],
    ["\uFEFF36\uFEFF", 36],
    ["\u00A036", 36],
    ["36\u00A0", 36],
    ["\uFEFF36\u00A0", 36],
    ["\u00A0+36\uFEFF", 36],
    ["\uFEFF-0\u00A0", 0],
    ["-2147483648", -2147483648],
    ["2147483647", 2147483647],
    ["+2147483647", 2147483647],
    ["\uFEFF-2147483648\u00A0", -2147483648],
    ["\u00A02147483647\uFEFF", 2147483647],
  ];
  for (const [input, want] of accepted) {
    const got = intParse(input);
    assert.deepStrictEqual(got, some(want), JSON.stringify(input));
    if (want === 0 && got._tag === "Some") {
      assert.equal(Object.is(got.value, 0), true, `normalized zero for ${JSON.stringify(input)}`);
    }
  }

  const ecmaSpaces = [
    0x0009, 0x000a, 0x000b, 0x000c, 0x000d, 0x0020, 0x00a0, 0x1680, 0x2000, 0x2001, 0x2002, 0x2003, 0x2004, 0x2005,
    0x2006, 0x2007, 0x2008, 0x2009, 0x200a, 0x2028, 0x2029, 0x202f, 0x205f, 0x3000, 0xfeff,
  ];
  for (const cp of ecmaSpaces) {
    const ch = String.fromCodePoint(cp);
    assert.deepStrictEqual(intParse(`${ch}36${ch}`), some(36), `U+${cp.toString(16).toUpperCase()}`);
  }

  const rejected = [
    "",
    "   ",
    "\t\n\v\f\r",
    "\uFEFF\u00A0",
    "+",
    "-",
    " + ",
    "\uFEFF-\u00A0",
    "++36",
    "--36",
    "+-36",
    "-+36",
    "\u008536",
    "36\u0085",
    "\u008536\u0085",
    "\u180E36",
    "\u200B36",
    "3 6",
    "3\t6",
    "3\n6",
    "3\u00A06",
    "3\uFEFF6",
    "+ 36",
    "- 36",
    "-2147483649",
    "2147483648",
    "+2147483648",
    "999999999999999999999999999999",
    "4.5",
    "36.0",
    "0x24",
    "1e2",
    "1_000",
    "٣٦",
  ];
  for (const input of rejected) {
    assert.deepStrictEqual(intParse(input), none, JSON.stringify(input));
  }
});


const repo = new URL("../../", import.meta.url);
const read = (path: string): string => readFileSync(new URL(path, repo), "utf8");

const exportsOf = async (path: string): Promise<Set<string>> => {
  const module: unknown = await import(path);
  return new Set(typeof module === "object" && module !== null ? Object.keys(module) : []);
};

test("core has every runtime helper compiled code calls", async () => {
  const core = await exportsOf("../src/core/index.ts");
  const dir = new URL("backends/js/src/codegen/js/", repo);
  // `kernel.rs` and `bridge.rs` emit only for Flick's own runtime.
  const codegen = (kernel: boolean): string =>
    readdirSync(dir)
      .filter((f) => f.endsWith(".rs") && (["kernel.rs", "bridge.rs"].includes(f)) === kernel)
      .map((f) => readFileSync(new URL(f, dir), "utf8"))
      .join("\n");
  const code = codegen(false);
  const helpers = [...code.matchAll(/rt\("(\w+)"\)/g)].map((m) => m[1] ?? "");
  assert.ok(helpers.length > 10);
  for (const helper of helpers) assert.ok(core.has(helper), `\`$rt.${helper}\``);
  const kernel = await exportsOf("../src/rt/index.ts");
  for (const [, helper] of codegen(true).matchAll(/rt\("(\w+)"\)/g)) assert.ok(helper && kernel.has(helper), `kernel \`$rt.${helper}\``);
  const { Effect } = await import("../src/core/index.ts");
  for (const [, method] of code.matchAll(/effect\("(\w+)"\)/g)) assert.ok(method && method in Effect, `Effect.${method}`);
});

test("the compiler's RT export map names real runtime exports", async () => {
  execFileSync(process.execPath, ["tools/rt-exports.mjs", "--check"], { cwd: fileURLToPath(repo) });
  const entries = read("runtime/rt_exports.txt")
    .trim()
    .split("\n")
    .map((line) => {
      const [spec = "", name = ""] = line.split(" ");
      assert.ok(spec.startsWith("@flick-lang/runtime/") && name, line);
      return { spec, name };
    });
  const modules = [...new Set(entries.map(({ spec }) => spec))];
  const exported = new Map(await Promise.all(modules.map(async (spec) => {
    const path = spec === "@flick-lang/runtime/rt"
      ? "../src/rt/index.ts"
      : `../src/${spec.slice("@flick-lang/runtime/".length)}.ts`;
    return [spec, await exportsOf(path)] as const;
  })));
  for (const { spec, name } of entries) assert.ok(exported.get(spec)?.has(name), `${spec} ${name}`);
});

test("no module of core imports Effect", () => {
  const dir = new URL("../src/core/", import.meta.url);
  for (const file of readdirSync(dir)) {
    assert.doesNotMatch(readFileSync(new URL(file, dir), "utf8"), /from "effect/, file);
  }
});
