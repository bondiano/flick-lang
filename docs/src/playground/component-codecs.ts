// Declarative Canonical ABI codecs for `flick:backend@0.1.0` WIT records and results.
import {
  alignTo,
  boolCodec,
  bytesCodec,
  enumCodec,
  listCodec,
  optionCodec,
  recordCodec,
  resultCodec,
  strCodec,
  u32Codec,
  u64Codec,
  type Codec,
} from "./component-abi.ts";
import type {
  Abi,
  AdapterArg,
  AdapterArgKind,
  AdapterWord,
  Artifact,
  BackendDiagnostic,
  BackendId,
  BackendPin,
  BuildRequest,
  BuildResult,
  Capability,
  Descriptor,
  EntryDecl,
  Environment,
  EnvironmentServices,
  ForeignBinding,
  HeaderRequest,
  HostExport,
  InlineForeign,
  InputDecl,
  InputRef,
  Launch,
  ModuleInput,
  NamedRange,
  OptionDescriptor,
  OptionValue,
  PackageTarget,
  Parameter,
  ProjectionDecl,
  RootHandler,
  RuntimeDependency,
  SourceDecl,
  Span,
  StepOutput,
  SymbolRef,
  TagAxis,
  TargetRequest,
  TargetSupport,
  Toolchain,
  ToolchainStep,
  TypeFormAttribute,
  ValueAbi,
  ValueAbiEntry,
  ValueAbiRefusal,
  VersionRange,
} from "./component-types.ts";

// ---------------------------------------------------------------------------
// WIT `flick:backend@0.1.0` type codecs
// ---------------------------------------------------------------------------

const profileCodec = enumCodec(["Debug", "Release"]);
const modeCodec = enumCodec(["PerModule", "WholeProgram"]);
const verbCodec = enumCodec(["Build", "Run", "Dev", "Test", "Inspect"]);
const valueKindCodec = enumCodec(["Flag", "Text", "Path"]);
const severityCodec = enumCodec(["Error", "Warning", "Note"]);
const inputKindCodec = enumCodec(["Source", "Foreign", "Interface", "TargetFacts", "Resource", "Lockfile"]);
const moduleRoleCodec = enumCodec(["Own", "Builtin", "External"]);
const artifactKindCodec = enumCodec([
  "Source",
  "SourceMap",
  "Declaration",
  "Object",
  "Executable",
  "Resource",
  "Metadata",
]);
const launchKindCodec = enumCodec(["Run", "Test"]);
const abiConversionCodec = enumCodec(["None", "Copy", "View"]);
const abiDirectionCodec = enumCodec(["In", "Out", "Both"]);

const environmentCodec = recordCodec<Environment>([
  ["backend", strCodec],
  ["platform", strCodec],
]);
const abiCodec = recordCodec<Abi>([
  ["name", strCodec],
  ["version", strCodec],
]);
const versionRangeCodec = recordCodec<VersionRange>([
  ["minimum", strCodec],
  ["maximum_exclusive", strCodec],
]);
const namedRangeCodec = recordCodec<NamedRange>([
  ["name", strCodec],
  ["versions", versionRangeCodec],
]);
const backendIdCodec = recordCodec<BackendId>([
  ["id", strCodec],
  ["version", strCodec],
]);
const backendPinCodec = recordCodec<BackendPin>([
  ["identity", backendIdCodec],
  ["sha256", strCodec],
]);
const capabilityCodec = recordCodec<Capability>([
  ["name", strCodec],
  ["version", strCodec],
]);
const optionValueCodec = recordCodec<OptionValue>([
  ["key", strCodec],
  ["value", strCodec],
]);
const tagAxisCodec = recordCodec<TagAxis>([
  ["name", strCodec],
  ["tags", listCodec(strCodec)],
  ["default", optionCodec(strCodec)],
  ["exclusive", boolCodec],
]);
const parameterCodec = recordCodec<Parameter>([
  ["name", strCodec],
  ["kind", valueKindCodec],
  ["multiple", boolCodec],
  ["required", boolCodec],
  ["help", strCodec],
]);
const optionDescriptorCodec = recordCodec<OptionDescriptor>([
  ["parameter", parameterCodec],
  ["default", optionCodec(strCodec)],
  ["implicit_value", optionCodec(strCodec)],
  ["choices", listCodec(strCodec)],
  ["verbs", listCodec(verbCodec)],
]);
const environmentServicesCodec = recordCodec<EnvironmentServices>([
  ["environment", environmentCodec],
  ["services", listCodec(strCodec)],
]);

const strListCodec = listCodec(strCodec);
const adapterArgKindCodec: Codec<AdapterArgKind> = {
  // variant { keyword(list<string>), param-index, uint, text, word }
  size: alignTo(alignTo(1, strListCodec.align) + strListCodec.size, strListCodec.align),
  align: strListCodec.align,
  lower(mem, ptr, value) {
    const payloadOffset = alignTo(1, strListCodec.align);
    if (typeof value === "object") {
      mem.view().setUint8(ptr, 0);
      strListCodec.lower(mem, ptr + payloadOffset, value.Keyword);
    } else {
      const idx = ["ParamIndex", "Uint", "Text", "Word"].indexOf(value);
      if (idx < 0) throw new Error(`Invalid AdapterArgKind: ${value}`);
      mem.view().setUint8(ptr, idx + 1);
    }
  },
  lift(mem, ptr) {
    const tag = mem.view().getUint8(ptr);
    if (tag === 0) return { Keyword: strListCodec.lift(mem, ptr + alignTo(1, strListCodec.align)) };
    const names = ["ParamIndex", "Uint", "Text", "Word"] as const;
    const name = names[tag - 1];
    if (name === undefined) throw new Error(`Invalid AdapterArgKind discriminant: ${tag}`);
    return name;
  },
};

const adapterArgCodec = recordCodec<AdapterArg>([
  ["name", strCodec],
  ["kind", adapterArgKindCodec],
]);
const adapterWordCodec = recordCodec<AdapterWord>([
  ["name", strCodec],
  ["args", listCodec(adapterArgCodec)],
  ["repeatable", boolCodec],
  ["doc", strCodec],
]);
const typeFormAttributeCodec = recordCodec<TypeFormAttribute>([
  ["name", strCodec],
  ["doc", strCodec],
]);
const valueAbiEntryCodec = recordCodec<ValueAbiEntry>([
  ["key", strCodec],
  ["flick_type", strCodec],
  ["host_type", strCodec],
  ["conversion", abiConversionCodec],
  ["direction", abiDirectionCodec],
  ["notes", strCodec],
]);
const valueAbiRefusalCodec = recordCodec<ValueAbiRefusal>([
  ["key", strCodec],
  ["reason", strCodec],
]);
const valueAbiCodec = recordCodec<ValueAbi>([
  ["abi", abiCodec],
  ["opaque_type", strCodec],
  ["catch_error_type", strCodec],
  ["catch_lang_item", optionCodec(strCodec)],
  ["entries", listCodec(valueAbiEntryCodec)],
  ["unsupported", listCodec(valueAbiRefusalCodec)],
  ["fallback_reason", strCodec],
  ["admits_copied_data", boolCodec],
]);

export const descriptorCodec = recordCodec<Descriptor>([
  ["identity", backendIdCodec],
  ["backend_api", versionRangeCodec],
  ["ir", versionRangeCodec],
  ["language", versionRangeCodec],
  ["runtime_abis", listCodec(namedRangeCodec)],
  ["environments", listCodec(environmentCodec)],
  ["tags", listCodec(strCodec)],
  ["axes", listCodec(tagAxisCodec)],
  ["profiles", listCodec(profileCodec)],
  ["modes", listCodec(modeCodec)],
  ["capabilities", listCodec(capabilityCodec)],
  ["options", listCodec(optionDescriptorCodec)],
  ["launch_runtimes", listCodec(strCodec)],
  ["host_tools", listCodec(strCodec)],
  ["projections", listCodec(abiCodec)],
  ["stages_by_class_option", optionCodec(optionValueCodec)],
  ["foreign_extension", optionCodec(strCodec)],
  ["foreign_suffixes", listCodec(strCodec)],
  ["handled_services", listCodec(environmentServicesCodec)],
  ["foreign_adapters", listCodec(adapterWordCodec)],
  ["type_form_attributes", listCodec(typeFormAttributeCodec)],
  ["value_abi", optionCodec(valueAbiCodec)],
]);

const spanCodec = recordCodec<Span>([
  ["file", u32Codec],
  ["start", u32Codec],
  ["end", u32Codec],
]);
const diagnosticCodec = recordCodec<BackendDiagnostic>([
  ["code", strCodec],
  ["severity", severityCodec],
  ["message", strCodec],
  ["span", optionCodec(spanCodec)],
  ["notes", listCodec(strCodec)],
]);
const targetSupportCodec = recordCodec<TargetSupport>([
  ["capabilities", listCodec(capabilityCodec)],
]);
const targetRequestCodec = recordCodec<TargetRequest>([
  ["backend_api", strCodec],
  ["ir_version", strCodec],
  ["language_version", strCodec],
  ["runtime_abis", listCodec(abiCodec)],
  ["environment", environmentCodec],
  ["tags", listCodec(strCodec)],
  ["host_package", optionCodec(strCodec)],
  ["profile", profileCodec],
  ["mode", modeCodec],
  ["required_capabilities", listCodec(capabilityCodec)],
  ["options", listCodec(optionValueCodec)],
]);
const inputDeclCodec = recordCodec<InputDecl>([
  ["id", strCodec],
  ["kind", inputKindCodec],
  ["sha256", strCodec],
  ["size", u64Codec],
]);
const sourceDeclCodec = recordCodec<SourceDecl>([
  ["name", strCodec],
  ["input", strCodec],
  ["line_starts", listCodec(u32Codec)],
]);
const moduleInputCodec = recordCodec<ModuleInput>([
  ["name", strCodec],
  ["role", moduleRoleCodec],
  ["package", optionCodec(strCodec)],
  ["output", strCodec],
  ["interface", optionCodec(strCodec)],
]);
const symbolCodec = recordCodec<SymbolRef>([
  ["module", strCodec],
  ["name", strCodec],
]);
const entryCodec = recordCodec<EntryDecl>([
  ["name", strCodec],
  ["symbol", symbolCodec],
  ["runner", optionCodec(strCodec)],
]);
const hostExportCodec = recordCodec<HostExport>([
  ["name", strCodec],
  ["symbol", symbolCodec],
  ["span", spanCodec],
]);
const headerRequestCodec = recordCodec<HeaderRequest>([
  ["enabled", boolCodec],
  ["format", optionCodec(strCodec)],
]);
const foreignBindingCodec = recordCodec<ForeignBinding>([
  ["module", strCodec],
  ["specifier", strCodec],
  ["resolved", strCodec],
  ["abi", abiCodec],
  ["inputs", listCodec(strCodec)],
]);
const inlineForeignCodec = recordCodec<InlineForeign>([
  ["module", strCodec],
  ["name", optionCodec(strCodec)],
  ["code", strCodec],
  ["span", spanCodec],
]);
const projectionCodec = recordCodec<ProjectionDecl>([
  ["module", strCodec],
  ["schema", abiCodec],
  ["input", strCodec],
]);
const packageTargetCodec = recordCodec<PackageTarget>([
  ["package", strCodec],
  ["environments", listCodec(environmentCodec)],
]);
const rootHandlerCodec = recordCodec<RootHandler>([
  ["environment", environmentCodec],
  ["service", symbolCodec],
  ["handler", symbolCodec],
]);
const toolchainCodec = recordCodec<Toolchain>([
  ["id", strCodec],
  ["version", strCodec],
  ["sha256", strCodec],
]);
const runtimeDependencyCodec = recordCodec<RuntimeDependency>([
  ["package", strCodec],
  ["version", strCodec],
  ["sha256", strCodec],
]);

export const buildRequestCodec = recordCodec<BuildRequest>([
  ["target", targetRequestCodec],
  ["backend", backendPinCodec],
  ["program", bytesCodec],
  ["module_root", optionCodec(strCodec)],
  ["modules", listCodec(moduleInputCodec)],
  ["sources", listCodec(sourceDeclCodec)],
  ["entries", listCodec(entryCodec)],
  ["test_roots", listCodec(symbolCodec)],
  ["foreign", listCodec(foreignBindingCodec)],
  ["inline_foreign", listCodec(inlineForeignCodec)],
  ["projections", listCodec(projectionCodec)],
  ["package_targets", listCodec(packageTargetCodec)],
  ["root_handlers", listCodec(rootHandlerCodec)],
  ["inputs", listCodec(inputDeclCodec)],
  ["toolchains", listCodec(toolchainCodec)],
  ["runtime_dependencies", listCodec(runtimeDependencyCodec)],
  ["declared_dependencies", optionCodec(listCodec(strCodec))],
  ["exports", listCodec(hostExportCodec)],
  ["headers", optionCodec(headerRequestCodec)],
]);

const artifactCodec = recordCodec<Artifact>([
  ["id", strCodec],
  ["path", strCodec],
  ["kind", artifactKindCodec],
  ["bytes", bytesCodec],
]);
const inputRefCodec: Codec<InputRef> = {
  size: alignTo(alignTo(1, strCodec.align) + strCodec.size, strCodec.align),
  align: strCodec.align,
  lower(mem, ptr, value) {
    const payloadOffset = alignTo(1, strCodec.align);
    if ("Source" in value) {
      mem.view().setUint8(ptr, 0);
      strCodec.lower(mem, ptr + payloadOffset, value.Source);
    } else {
      mem.view().setUint8(ptr, 1);
      strCodec.lower(mem, ptr + payloadOffset, value.Artifact);
    }
  },
  lift(mem, ptr) {
    const payloadOffset = alignTo(1, strCodec.align);
    const tag = mem.view().getUint8(ptr);
    const val = strCodec.lift(mem, ptr + payloadOffset);
    if (tag === 0) return { Source: val };
    if (tag === 1) return { Artifact: val };
    throw new Error(`Invalid InputRef discriminant: ${tag}`);
  },
};
const stepOutputCodec = recordCodec<StepOutput>([
  ["id", strCodec],
  ["path", strCodec],
  ["kind", artifactKindCodec],
]);
const toolchainStepCodec = recordCodec<ToolchainStep>([
  ["id", strCodec],
  ["dependencies", listCodec(strCodec)],
  ["toolchain", toolchainCodec],
  ["arguments", listCodec(strCodec)],
  ["environment", listCodec(optionValueCodec)],
  ["inputs", listCodec(inputRefCodec)],
  ["outputs", listCodec(stepOutputCodec)],
]);
const launchCodec = recordCodec<Launch>([
  ["name", strCodec],
  ["kind", launchKindCodec],
  ["artifact", strCodec],
  ["runtime", optionCodec(toolchainCodec)],
  ["runtime_arguments", listCodec(strCodec)],
  ["arguments", listCodec(strCodec)],
  ["environment", listCodec(optionValueCodec)],
]);
const buildResultCodec = recordCodec<BuildResult>([
  ["artifacts", listCodec(artifactCodec)],
  ["steps", listCodec(toolchainStepCodec)],
  ["runtime_dependencies", listCodec(runtimeDependencyCodec)],
  ["launches", listCodec(launchCodec)],
  ["diagnostics", listCodec(diagnosticCodec)],
]);

export const checkTargetResultCodec = resultCodec(targetSupportCodec, listCodec(diagnosticCodec));
export const generateResultCodec = resultCodec(buildResultCodec, listCodec(diagnosticCodec));
