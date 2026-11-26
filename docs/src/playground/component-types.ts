// Browser & Node host for the `flick:backend@0.1.0` WebAssembly Component world.
//
// Security boundary:
// - Verifies the SHA-256 of component bytes via WebCrypto before compiling.
// - Extracts the single core module from the Wasm Component Binary Format (section id 1)
//   and rejects any import outside `flick:backend/inputs@0.1.0#read`.
// - Instantiates a fresh WebAssembly.Instance per operation so mutable guest linear
//   memory is never retained across calls.
// - Pre-verifies every declared input's byte length and SHA-256 before entering WASM,
//   returning `InputError::Undeclared` (0) for any undeclared input id.
// - Grants zero filesystem, network, clock, random, storage, or DOM authority.

export interface BackendId {
  readonly id: string;
  readonly version: string;
}

export interface BackendPin {
  readonly identity: BackendId;
  readonly sha256: string;
}

export interface VersionRange {
  readonly minimum: string;
  readonly maximum_exclusive: string;
}

export interface NamedRange {
  readonly name: string;
  readonly versions: VersionRange;
}

export interface Environment {
  readonly backend: string;
  readonly platform: string;
}

export interface Abi {
  readonly name: string;
  readonly version: string;
}

export interface Capability {
  readonly name: string;
  readonly version: string;
}

export interface OptionValue {
  readonly key: string;
  readonly value: string;
}

export interface TagAxis {
  readonly name: string;
  readonly tags: ReadonlyArray<string>;
  readonly default: string | null;
  readonly exclusive: boolean;
}

export interface Parameter {
  readonly name: string;
  readonly kind: "Flag" | "Text" | "Path";
  readonly multiple: boolean;
  readonly required: boolean;
  readonly help: string;
}

export interface OptionDescriptor {
  readonly parameter: Parameter;
  readonly default: string | null;
  readonly implicit_value: string | null;
  readonly choices: ReadonlyArray<string>;
  readonly verbs: ReadonlyArray<"Build" | "Run" | "Dev" | "Test" | "Inspect">;
}

export interface EnvironmentServices {
  readonly environment: Environment;
  readonly services: ReadonlyArray<string>;
}

export type AdapterArgKind =
  | { readonly Keyword: ReadonlyArray<string> }
  | "ParamIndex"
  | "Uint"
  | "Text"
  | "Word";

export interface AdapterArg {
  readonly name: string;
  readonly kind: AdapterArgKind;
}

export interface AdapterWord {
  readonly name: string;
  readonly args: ReadonlyArray<AdapterArg>;
  readonly repeatable: boolean;
  readonly doc: string;
}

export interface TypeFormAttribute {
  readonly name: string;
  readonly doc: string;
}

export interface ValueAbiEntry {
  readonly key: string;
  readonly flick_type: string;
  readonly host_type: string;
  readonly conversion: "None" | "Copy" | "View";
  readonly direction: "In" | "Out" | "Both";
  readonly notes: string;
}

export interface ValueAbiRefusal {
  readonly key: string;
  readonly reason: string;
}

export interface ValueAbi {
  readonly abi: Abi;
  readonly opaque_type: string;
  readonly catch_error_type: string;
  readonly catch_lang_item: string | null;
  readonly entries: ReadonlyArray<ValueAbiEntry>;
  readonly unsupported: ReadonlyArray<ValueAbiRefusal>;
  readonly fallback_reason: string;
  readonly admits_copied_data: boolean;
}

export interface Descriptor {
  readonly identity: BackendId;
  readonly backend_api: VersionRange;
  readonly ir: VersionRange;
  readonly language: VersionRange;
  readonly runtime_abis: ReadonlyArray<NamedRange>;
  readonly environments: ReadonlyArray<Environment>;
  readonly tags: ReadonlyArray<string>;
  readonly axes: ReadonlyArray<TagAxis>;
  readonly profiles: ReadonlyArray<"Debug" | "Release">;
  readonly modes: ReadonlyArray<"PerModule" | "WholeProgram">;
  readonly capabilities: ReadonlyArray<Capability>;
  readonly options: ReadonlyArray<OptionDescriptor>;
  readonly launch_runtimes: ReadonlyArray<string>;
  readonly host_tools: ReadonlyArray<string>;
  readonly projections: ReadonlyArray<Abi>;
  readonly stages_by_class_option: OptionValue | null;
  readonly foreign_extension: string | null;
  readonly foreign_suffixes: ReadonlyArray<string>;
  readonly handled_services: ReadonlyArray<EnvironmentServices>;
  readonly foreign_adapters: ReadonlyArray<AdapterWord>;
  readonly type_form_attributes: ReadonlyArray<TypeFormAttribute>;
  readonly value_abi: ValueAbi | null;
}

export interface Span {
  readonly file: number;
  readonly start: number;
  readonly end: number;
}

export interface BackendDiagnostic {
  readonly code: string;
  readonly severity: "Error" | "Warning" | "Note";
  readonly message: string;
  readonly span: Span | null;
  readonly notes: ReadonlyArray<string>;
}

export interface TargetRequest {
  readonly backend_api: string;
  readonly ir_version: string;
  readonly language_version: string;
  readonly runtime_abis: ReadonlyArray<Abi>;
  readonly environment: Environment;
  readonly tags: ReadonlyArray<string>;
  readonly host_package: string | null;
  readonly profile: "Debug" | "Release";
  readonly mode: "PerModule" | "WholeProgram";
  readonly required_capabilities: ReadonlyArray<Capability>;
  readonly options: ReadonlyArray<OptionValue>;
}

export interface TargetSupport {
  readonly capabilities: ReadonlyArray<Capability>;
}

export interface InputDecl {
  readonly id: string;
  readonly kind: "Source" | "Foreign" | "Interface" | "TargetFacts" | "Resource" | "Lockfile";
  readonly sha256: string;
  readonly size: number;
}

export interface SourceDecl {
  readonly name: string;
  readonly input: string;
  readonly line_starts: ReadonlyArray<number>;
}

export interface ModuleInput {
  readonly name: string;
  readonly role: "Own" | "Builtin" | "External";
  readonly package: string | null;
  readonly output: string;
  readonly interface: string | null;
}

export interface SymbolRef {
  readonly module: string;
  readonly name: string;
}

export interface EntryDecl {
  readonly name: string;
  readonly symbol: SymbolRef;
  readonly runner: string | null;
}

export interface HostExport {
  readonly name: string;
  readonly symbol: SymbolRef;
  readonly span: Span;
}

export interface HeaderRequest {
  readonly enabled: boolean;
  readonly format: string | null;
}

export interface ForeignBinding {
  readonly module: string;
  readonly specifier: string;
  readonly resolved: string;
  readonly abi: Abi;
  readonly inputs: ReadonlyArray<string>;
}

export interface InlineForeign {
  readonly module: string;
  readonly name: string | null;
  readonly code: string;
  readonly span: Span;
}

export interface ProjectionDecl {
  readonly module: string;
  readonly schema: Abi;
  readonly input: string;
}

export interface PackageTarget {
  readonly package: string;
  readonly environments: ReadonlyArray<Environment>;
}

export interface RootHandler {
  readonly environment: Environment;
  readonly service: SymbolRef;
  readonly handler: SymbolRef;
}

export interface Toolchain {
  readonly id: string;
  readonly version: string;
  readonly sha256: string;
}

export interface RuntimeDependency {
  readonly package: string;
  readonly version: string;
  readonly sha256: string;
}

export interface BuildRequest {
  readonly target: TargetRequest;
  readonly backend: BackendPin;
  readonly program: Uint8Array;
  readonly module_root: string | null;
  readonly modules: ReadonlyArray<ModuleInput>;
  readonly sources: ReadonlyArray<SourceDecl>;
  readonly entries: ReadonlyArray<EntryDecl>;
  readonly test_roots: ReadonlyArray<SymbolRef>;
  readonly foreign: ReadonlyArray<ForeignBinding>;
  readonly inline_foreign: ReadonlyArray<InlineForeign>;
  readonly projections: ReadonlyArray<ProjectionDecl>;
  readonly package_targets: ReadonlyArray<PackageTarget>;
  readonly root_handlers: ReadonlyArray<RootHandler>;
  readonly inputs: ReadonlyArray<InputDecl>;
  readonly toolchains: ReadonlyArray<Toolchain>;
  readonly runtime_dependencies: ReadonlyArray<RuntimeDependency>;
  readonly declared_dependencies: ReadonlyArray<string> | null;
  readonly exports: ReadonlyArray<HostExport>;
  readonly headers: HeaderRequest | null;
}

export type ArtifactKind =
  | "Source"
  | "SourceMap"
  | "Declaration"
  | "Object"
  | "Executable"
  | "Resource"
  | "Metadata";

export interface Artifact {
  readonly id: string;
  readonly path: string;
  readonly kind: ArtifactKind;
  readonly bytes: Uint8Array;
}

export type InputRef = { readonly Source: string } | { readonly Artifact: string };

export interface StepOutput {
  readonly id: string;
  readonly path: string;
  readonly kind: ArtifactKind;
}

export interface ToolchainStep {
  readonly id: string;
  readonly dependencies: ReadonlyArray<string>;
  readonly toolchain: Toolchain;
  readonly arguments: ReadonlyArray<string>;
  readonly environment: ReadonlyArray<OptionValue>;
  readonly inputs: ReadonlyArray<InputRef>;
  readonly outputs: ReadonlyArray<StepOutput>;
}

export interface Launch {
  readonly name: string;
  readonly kind: "Run" | "Test";
  readonly artifact: string;
  readonly runtime: Toolchain | null;
  readonly runtime_arguments: ReadonlyArray<string>;
  readonly arguments: ReadonlyArray<string>;
  readonly environment: ReadonlyArray<OptionValue>;
}

export interface BuildResult {
  readonly artifacts: ReadonlyArray<Artifact>;
  readonly steps: ReadonlyArray<ToolchainStep>;
  readonly runtime_dependencies: ReadonlyArray<RuntimeDependency>;
  readonly launches: ReadonlyArray<Launch>;
  readonly diagnostics: ReadonlyArray<BackendDiagnostic>;
}

export type BackendCallResult<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly diagnostics: ReadonlyArray<BackendDiagnostic> };

export const base64ToBytes = (b64: string): Uint8Array => {
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
};

export const bytesToBase64 = (bytes: Uint8Array): string => {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
};

export const sha256Hex = async (bytes: Uint8Array): Promise<string> => {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  const digest = await globalThis.crypto.subtle.digest("SHA-256", copy);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
};
