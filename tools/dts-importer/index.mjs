#!/usr/bin/env node
// Reads the declarations of an installed package with the TypeScript checker
// and prints the shapes of its value exports as JSON, for `flick ffi import`.
// Input on stdin: {"root": "<project dir>", "package": "<name>"}.
// The shapes are already reduced by the checker: aliases, mapped and
// conditional types are resolved; whatever stays outside the forms below is
// `{"k": "other"}` with its TypeScript text, for the importer to refuse.
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";

import ts from "typescript";

const MAX_DEPTH = 8;

const fail = (message) => {
  process.stderr.write(`${message}\n`);
  process.exit(1);
};

const request = JSON.parse(readFileSync(0, "utf8"));
const options = {
  module: ts.ModuleKind.NodeNext,
  moduleResolution: ts.ModuleResolutionKind.NodeNext,
  target: ts.ScriptTarget.ESNext,
  strict: true,
  noEmit: true,
  skipLibCheck: true,
};
const from = path.join(request.root, "__flick_ffi_import__.mts");
const resolved = ts.resolveModuleName(request.package, from, options, ts.sys, undefined, undefined, ts.ModuleKind.ESNext)
  .resolvedModule;
if (resolved === undefined) fail(`cannot resolve the declarations of \`${request.package}\` from ${request.root}`);
if (!/\.d\.[cm]?ts$/.test(resolved.resolvedFileName)) {
  fail(`\`${request.package}\` resolves to ${resolved.resolvedFileName}, which is not a declaration file`);
}

const program = ts.createProgram([resolved.resolvedFileName], options);
const checker = program.getTypeChecker();
const source = program.getSourceFile(resolved.resolvedFileName);
const moduleSymbol = source && checker.getSymbolAtLocation(source);
if (moduleSymbol === undefined) fail(`${resolved.resolvedFileName} declares no module`);

const text = (type) => checker.typeToString(type, undefined, ts.TypeFormatFlags.NoTruncation);
const other = (type) => ({ k: "other", text: text(type) });

const HANDLES = new Set(["Date", "RegExp", "Error", "AbortSignal", "URL", "Uint8Array", "ArrayBuffer"]);

/** The shape of a type, `depth` levels deep at most. */
const shape = (type, depth = 0) => {
  if (depth > MAX_DEPTH) return other(type);
  const f = type.flags;
  if (f & ts.TypeFlags.Any) return { k: "any" };
  if (f & ts.TypeFlags.Unknown) return { k: "unknown" };
  if (f & ts.TypeFlags.Boolean) return { k: "boolean" };
  if (f & ts.TypeFlags.String) return { k: "string" };
  if (f & ts.TypeFlags.Number) return { k: "number" };
  if (f & ts.TypeFlags.BigInt) return { k: "bigint" };
  if (f & ts.TypeFlags.Void) return { k: "void" };
  if (f & ts.TypeFlags.Undefined) return { k: "undefined" };
  if (f & ts.TypeFlags.Null) return { k: "null" };
  if (f & ts.TypeFlags.Never) return { k: "never" };
  if (f & (ts.TypeFlags.StringLiteral | ts.TypeFlags.NumberLiteral | ts.TypeFlags.BooleanLiteral)) {
    return { k: "literal", text: text(type) };
  }
  if (f & ts.TypeFlags.TypeParameter) {
    const constraint = checker.getBaseConstraintOfType(type);
    const bounded = constraint !== undefined && !(constraint.flags & ts.TypeFlags.Unknown) && constraint !== type;
    return bounded ? { k: "param", name: type.symbol.name, bound: text(constraint) } : { k: "param", name: type.symbol.name };
  }
  if (f & ts.TypeFlags.Union) return { k: "union", types: type.types.map((t) => shape(t, depth + 1)) };
  if (!(f & ts.TypeFlags.Object)) return other(type);
  if (checker.isTupleType(type)) {
    const target = type.target;
    if (target.hasRestElement || target.minLength !== target.fixedLength) return other(type);
    return { k: "tuple", elements: checker.getTypeArguments(type).map((t) => shape(t, depth + 1)) };
  }
  if (checker.isArrayType(type)) return { k: "array", element: shape(checker.getTypeArguments(type)[0], depth + 1) };
  const name = type.symbol?.name;
  // an interface's own type arguments end with its implicit `this`
  const args = type.aliasTypeArguments
    ?? (type.objectFlags & ts.ObjectFlags.Reference ? checker.getTypeArguments(type).filter((t) => !t.isThisType) : []);
  if ((name === "Promise" || name === "PromiseLike") && args.length === 1) {
    return { k: "promise", type: shape(args[0], depth + 1) };
  }
  const calls = type.getCallSignatures();
  if (calls.length > 0 && type.getProperties().length === 0) {
    return { k: "function", signatures: calls.map((s) => signature(s, depth + 1)) };
  }
  const isClass = type.symbol !== undefined && (type.symbol.flags & ts.SymbolFlags.Class) !== 0;
  if (name !== undefined && (isClass || HANDLES.has(name))) {
    return args.length === 0 ? { k: "handle", name } : other(type);
  }
  // a generic object keeps its type arguments only as text: it stays outside the subset
  if (calls.length === 0 && type.getConstructSignatures().length === 0 && name !== undefined && args.length === 0) {
    const named = type.aliasSymbol?.name ?? (name === "__type" || name === "__object" ? null : name);
    return { k: "object", name: named, text: text(type) };
  }
  return other(type);
};

const parameter = (symbol, depth) => {
  const declaration = symbol.valueDeclaration;
  const optional = declaration !== undefined && ts.isParameter(declaration)
    && (declaration.questionToken !== undefined || declaration.initializer !== undefined);
  const rest = declaration !== undefined && ts.isParameter(declaration) && declaration.dotDotDotToken !== undefined;
  let type = checker.getTypeOfSymbol(symbol);
  // an optional parameter's type carries `| undefined`; the flag says it already
  if (optional && type.flags & ts.TypeFlags.Union) {
    const kept = type.types.filter((t) => !(t.flags & ts.TypeFlags.Undefined));
    if (kept.length === 1) type = kept[0];
  }
  return { name: symbol.name, optional, rest, type: shape(type, depth) };
};

const signature = (sig, depth = 0) => ({
  params: sig.getParameters().map((p) => parameter(p, depth)),
  this: sig.thisParameter !== undefined,
  typeParams: (sig.getTypeParameters() ?? []).map((t) => t.symbol.name),
  result: shape(checker.getReturnTypeOfSignature(sig), depth),
});

const fingerprint = (value) => `sha256:${createHash("sha256").update(JSON.stringify(value)).digest("hex")}`;

const describe = (exported) => {
  const symbol = exported.flags & ts.SymbolFlags.Alias ? checker.getAliasedSymbol(exported) : exported;
  const declaration = symbol.valueDeclaration ?? symbol.declarations?.[0];
  // `export default function delay`: bound in Flick by the function's own name
  const local = exported.name === "default" && declaration?.name !== undefined ? declaration.name.getText() : exported.name;
  if (!(symbol.flags & ts.SymbolFlags.Value)) return null;
  const base = { name: exported.name, local };
  if (symbol.flags & ts.SymbolFlags.Class) return { ...base, kind: "class" };
  if (symbol.flags & ts.SymbolFlags.Enum) return { ...base, kind: "value", type: { k: "other", text: "enum" } };
  const type = checker.getTypeOfSymbol(symbol);
  const calls = type.getCallSignatures();
  if (calls.length > 0 && symbol.flags & (ts.SymbolFlags.Function | ts.SymbolFlags.Variable)) {
    return { ...base, kind: "function", signatures: calls.map((s) => signature(s)) };
  }
  return { ...base, kind: "value", type: shape(type) };
};

const exports = checker
  .getExportsOfModule(moduleSymbol)
  .map(describe)
  .filter((e) => e !== null)
  .map((e) => ({ ...e, fingerprint: fingerprint({ ...e, local: undefined }) }))
  .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));

process.stdout.write(`${JSON.stringify({
  package: resolved.packageId?.name ?? request.package,
  version: resolved.packageId?.version ?? null,
  declarations: resolved.packageId === undefined
    ? path.basename(resolved.resolvedFileName)
    : `${resolved.packageId.name}/${resolved.packageId.subModuleName}`,
  exports,
})}\n`);
