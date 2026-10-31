import type ts from "typescript";

export interface PackageId {
  readonly name: string;
  readonly version: string;
  readonly subModuleName: string;
}

export declare const resolve: (pkg: string, from: string) => ts.ResolvedModuleFull | undefined;
export declare const resolvedFile: (r: ts.ResolvedModuleFull) => string;
export declare const resolvedPackage: (r: ts.ResolvedModuleFull) => PackageId | undefined;

export declare const program: (file: string) => ts.Program;
export declare const checker: (p: ts.Program) => ts.TypeChecker;
export declare const moduleOf: (p: ts.Program, file: string) => ts.Symbol | undefined;
export declare const exportsOf: (c: ts.TypeChecker, s: ts.Symbol) => ReadonlyArray<ts.Symbol>;
export declare const aliased: (c: ts.TypeChecker, s: ts.Symbol) => ts.Symbol;
export declare const typeOfSymbol: (c: ts.TypeChecker, s: ts.Symbol) => ts.Type;
export declare const typeText: (c: ts.TypeChecker, t: ts.Type) => string;
export declare const baseConstraint: (c: ts.TypeChecker, t: ts.Type) => ts.Type | undefined;
export declare const isTuple: (c: ts.TypeChecker, t: ts.Type) => boolean;
export declare const isArray: (c: ts.TypeChecker, t: ts.Type) => boolean;
export declare const typeArguments: (c: ts.TypeChecker, t: ts.Type) => ReadonlyArray<ts.Type>;
export declare const returnType: (c: ts.TypeChecker, s: ts.Signature) => ts.Type;

export declare const symbolName: (s: ts.Symbol) => string;
export declare const symbolHas: (flag: keyof typeof ts.SymbolFlags, s: ts.Symbol) => boolean;
export declare const valueDeclaration: (s: ts.Symbol) => ts.Declaration | undefined;
export declare const declarations: (s: ts.Symbol) => ReadonlyArray<ts.Declaration>;

export declare const typeHas: (flag: keyof typeof ts.TypeFlags, t: ts.Type) => boolean;
export declare const objectHas: (flag: keyof typeof ts.ObjectFlags, t: ts.Type) => boolean;
export declare const same: (a: ts.Type, b: ts.Type) => boolean;
export declare const isThis: (t: ts.Type) => boolean;
export declare const members: (t: ts.UnionType) => ReadonlyArray<ts.Type>;
export declare const typeSymbol: (t: ts.Type) => ts.Symbol | undefined;
export declare const aliasSymbol: (t: ts.Type) => ts.Symbol | undefined;
export declare const aliasArguments: (t: ts.Type) => ReadonlyArray<ts.Type> | undefined;
export declare const tupleHasRest: (t: ts.TupleTypeReference) => boolean;
export declare const tupleMinLength: (t: ts.TupleTypeReference) => number;
export declare const tupleFixedLength: (t: ts.TupleTypeReference) => number;
export declare const callSignatures: (t: ts.Type) => ReadonlyArray<ts.Signature>;
export declare const constructSignatures: (t: ts.Type) => ReadonlyArray<ts.Signature>;
export declare const propertyCount: (t: ts.Type) => number;

export declare const parameters: (s: ts.Signature) => ReadonlyArray<ts.Symbol>;
export declare const hasThis: (s: ts.Signature) => boolean;
export declare const typeParameters: (s: ts.Signature) => ReadonlyArray<ts.TypeParameter>;

export declare const declarationIs: (kind: string, d: ts.Node) => boolean;
export declare const declarationHas: (field: string, d: ts.Node) => boolean;
export declare const declarationName: (d: ts.Declaration) => string | undefined;
export declare const declarationSite: (d: ts.Declaration) => string;
export declare const printed: (d: ts.Node) => string;

export declare const sha256: (text: string) => string;
export declare const exec: (command: string, args: ReadonlyArray<string>) => void;
