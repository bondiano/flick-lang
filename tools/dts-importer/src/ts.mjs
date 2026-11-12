// Primitive queries over the TypeScript compiler for `Ts.fl`: each one reads
// one fact of a program, checker, type, symbol, signature or declaration,
// which stay opaque handles on the Flick side. What the facts mean lives in
// the Flick modules.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";

import ts from "typescript";

const options = {
  module: ts.ModuleKind.NodeNext,
  moduleResolution: ts.ModuleResolutionKind.NodeNext,
  target: ts.ScriptTarget.ESNext,
  strict: true,
  noEmit: true,
  skipLibCheck: true,
};

const printer = ts.createPrinter({ removeComments: true, newLine: ts.NewLineKind.LineFeed });

export const resolve = (pkg, from) =>
  ts.resolveModuleName(pkg, from, options, ts.sys, undefined, undefined, ts.ModuleKind.ESNext).resolvedModule;
export const resolvedFile = (r) => r.resolvedFileName;
export const resolvedPackage = (r) =>
  r.packageId && { name: r.packageId.name, version: r.packageId.version, subModuleName: r.packageId.subModuleName };

export const program = (file) => ts.createProgram([file], options);
export const checker = (p) => p.getTypeChecker();
export const moduleOf = (p, file) => {
  const source = p.getSourceFile(file);
  return source && p.getTypeChecker().getSymbolAtLocation(source);
};
export const exportsOf = (c, s) => c.getExportsOfModule(s);
export const aliased = (c, s) => c.getAliasedSymbol(s);
export const typeOfSymbol = (c, s) => c.getTypeOfSymbol(s);
export const typeText = (c, t) => c.typeToString(t, undefined, ts.TypeFormatFlags.NoTruncation);
export const baseConstraint = (c, t) => c.getBaseConstraintOfType(t);
export const isTuple = (c, t) => c.isTupleType(t);
export const isArray = (c, t) => c.isArrayType(t);
export const typeArguments = (c, t) => c.getTypeArguments(t);
export const returnType = (c, s) => c.getReturnTypeOfSignature(s);

export const symbolName = (s) => s.name;
export const symbolHas = (flag, s) => (s.flags & ts.SymbolFlags[flag]) !== 0;
export const valueDeclaration = (s) => s.valueDeclaration;
export const declarations = (s) => s.declarations ?? [];

export const typeHas = (flag, t) => (t.flags & ts.TypeFlags[flag]) !== 0;
export const objectHas = (flag, t) => ((t.objectFlags ?? 0) & ts.ObjectFlags[flag]) !== 0;
export const same = (a, b) => a === b;
export const isThis = (t) => t.isThisType === true;
export const members = (t) => t.types;
export const typeSymbol = (t) => t.symbol;
export const aliasSymbol = (t) => t.aliasSymbol;
export const aliasArguments = (t) => t.aliasTypeArguments;
export const tupleHasRest = (t) => t.target.hasRestElement;
export const tupleMinLength = (t) => t.target.minLength;
export const tupleFixedLength = (t) => t.target.fixedLength;
export const callSignatures = (t) => t.getCallSignatures();
export const constructSignatures = (t) => t.getConstructSignatures();
export const propertyCount = (t) => t.getProperties().length;

export const parameters = (s) => s.getParameters();
export const hasThis = (s) => s.thisParameter !== undefined;
export const typeParameters = (s) => s.getTypeParameters() ?? [];

/** `ts.is<kind>(d)`, as `Parameter` or `SourceFile`. */
export const declarationIs = (kind, d) => ts[`is${kind}`](d);
export const declarationHas = (field, d) => d[field] !== undefined;
export const declarationName = (d) => d.name?.getText();
export const declarationSite = (d) => `${d.getSourceFile().fileName}:${d.pos}`;
/**
 * The name the global scope has the type by, `Date` or
 * `Intl.DateTimeFormatOptions`: its own, or through the namespaces it is
 * declared in. A type a module declares has none.
 */
export const globalName = (c, t) => {
  const symbol = t.aliasSymbol ?? t.symbol;
  if (symbol === undefined) return undefined;
  const name = c.getFullyQualifiedName(symbol);
  // a member of a module is named through the module's file, in quotes
  if (name.startsWith('"')) return undefined;
  const [root, ...members] = name.split(".");
  let found = c.resolveName(root, undefined, ts.SymbolFlags.Type | ts.SymbolFlags.Namespace, false);
  for (const member of members) {
    found = found && c.getExportsOfModule(found).find((e) => e.name === member);
  }
  return found === symbol ? name : undefined;
};
export const printed = (d) => printer.printNode(ts.EmitHint.Unspecified, d, d.getSourceFile());

export const sha256 = (text) => createHash("sha256").update(text).digest("hex");

/** Runs the command, its stdout discarded; a failure throws. */
export const exec = (command, args) => {
  execFileSync(command, args, { stdio: ["ignore", "ignore", "inherit"] });
};
