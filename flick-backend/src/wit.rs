//! Typed conversions for a Wasmtime host. Guest and native records come
//! from WIT; these mappings are checked against both generated definitions.

/// Map the records of the backend world to and from Wasmtime's bindings.
/// Used by the host, never as an alternative public ABI.
#[macro_export]
#[doc(hidden)]
macro_rules! wit_conversions {
    ($($wit:ident)::+) => { $crate::wit_conversions!(@impl [$($wit)::+] $); };
    (@impl [$($wit:ident)::+] $d:tt) => {
        mod convert {
            use super::$($wit)::+ as w;
            trait ToWire { type Wire; fn to_wire(&self) -> Self::Wire; }
            trait IntoNative { type Native; fn into_native(self) -> Self::Native; }
            macro_rules! scalars {
                ($d($d ty:ty),*) => {$d(
                    impl ToWire for $d ty { type Wire = Self; fn to_wire(&self) -> Self { *self } }
                    impl IntoNative for $d ty { type Native = Self; fn into_native(self) -> Self { self } }
                )*};
            }
            scalars!(u8, u32, u64, bool);
            impl ToWire for String { type Wire = String; fn to_wire(&self) -> String { self.clone() } }
            impl IntoNative for String { type Native = String; fn into_native(self) -> String { self } }
            impl<T: ToWire> ToWire for Vec<T> {
                type Wire = Vec<T::Wire>;
                fn to_wire(&self) -> Self::Wire { self.iter().map(ToWire::to_wire).collect() }
            }
            impl<T: IntoNative> IntoNative for Vec<T> {
                type Native = Vec<T::Native>;
                fn into_native(self) -> Self::Native { self.into_iter().map(IntoNative::into_native).collect() }
            }
            impl<T: ToWire> ToWire for Option<T> {
                type Wire = Option<T::Wire>;
                fn to_wire(&self) -> Self::Wire { self.as_ref().map(ToWire::to_wire) }
            }
            impl<T: IntoNative> IntoNative for Option<T> {
                type Native = Option<T::Native>;
                fn into_native(self) -> Self::Native { self.map(IntoNative::into_native) }
            }
            macro_rules! records {
                ($d($d name:ident { $d($d field:ident),* })*) => {$d(
                    impl ToWire for $crate::$d name {
                        type Wire = w::$d name;
                        fn to_wire(&self) -> Self::Wire { w::$d name { $d($d field: self.$d field.to_wire()),* } }
                    }
                    impl IntoNative for w::$d name {
                        type Native = $crate::$d name;
                        fn into_native(self) -> Self::Native { $crate::$d name { $d($d field: self.$d field.into_native()),* } }
                    }
                )*};
            }
            macro_rules! enums {
                ($d($d name:ident { $d($d case:ident),* })*) => {$d(
                    impl ToWire for $crate::$d name {
                        type Wire = w::$d name;
                        fn to_wire(&self) -> Self::Wire { match self { $d($crate::$d name::$d case => w::$d name::$d case),* } }
                    }
                    impl IntoNative for w::$d name {
                        type Native = $crate::$d name;
                        fn into_native(self) -> Self::Native { match self { $d(w::$d name::$d case => $crate::$d name::$d case),* } }
                    }
                )*};
            }
            records! {
            Environment { backend, platform }
            Abi { name, version }
            VersionRange { minimum, maximum_exclusive }
            NamedRange { name, versions }
            BackendId { id, version }
            BackendPin { identity, sha256 }
            Capability { name, version }
            Descriptor { identity, backend_api, ir, language, runtime_abis, environments, profiles, modes, capabilities }
            OptionValue { key, value }
            TargetRequest { backend_api, ir_version, language_version, runtime_abis, environment, profile, mode, required_capabilities, options }
            TargetSupport { capabilities }
            Span { file, start, end }
            Diagnostic { code, severity, message, span, notes }
            Input { id, kind, sha256, size }
            Source { name, input, line_starts }
            ModuleInput { name, role, package, output, interface }
            Symbol { module, name }
            Entry { name, symbol, runner }
            ForeignBinding { module, specifier, resolved, abi, inputs }
            Projection { module, schema, input }
            PackageTarget { package, environments }
            RootHandler { environment, service, handler }
            BuildRequest { target, backend, program, module_root, modules, sources, entries, test_roots, foreign, projections, package_targets, root_handlers, inputs, toolchains, runtime_dependencies }
            Artifact { id, path, kind, bytes }
            Toolchain { id, version, sha256 }
            StepOutput { id, path, kind }
            ToolchainStep { id, dependencies, toolchain, arguments, environment, inputs, outputs }
            RuntimeDependency { package, version, sha256 }
            Launch { name, kind, artifact, runtime, arguments }
            BuildResult { artifacts, steps, runtime_dependencies, launches, diagnostics }
            }
            enums! {
            Profile { Debug, Release }
            Mode { PerModule, WholeProgram }
            Severity { Error, Warning, Note }
            InputKind { Source, Foreign, Interface, TargetFacts, Resource, Lockfile }
            ModuleRole { Own, Builtin, External }
            ArtifactKind { Source, SourceMap, Declaration, Object, Executable, Resource, Metadata }
            LaunchKind { Run, Test }
            InputError { Undeclared, Unavailable, DigestMismatch }
            }
            impl ToWire for $crate::InputRef {
                type Wire = w::InputRef;
                fn to_wire(&self) -> Self::Wire {
                    match self {
                        Self::Source(s) => w::InputRef::Source(s.clone()),
                        Self::Artifact(s) => w::InputRef::Artifact(s.clone()),
                    }
                }
            }
            impl IntoNative for w::InputRef {
                type Native = $crate::InputRef;
                fn into_native(self) -> Self::Native {
                    match self {
                        Self::Source(s) => $crate::InputRef::Source(s),
                        Self::Artifact(s) => $crate::InputRef::Artifact(s),
                    }
                }
            }
            pub fn request(request: &$crate::BuildRequest) -> w::BuildRequest { request.to_wire() }
            pub fn descriptor(descriptor: w::Descriptor) -> $crate::Descriptor { descriptor.into_native() }
            pub fn input_error(error: $crate::InputError) -> w::InputError { error.to_wire() }
            pub fn diagnostics(diagnostics: Vec<w::Diagnostic>) -> Vec<$crate::Diagnostic> { diagnostics.into_native() }
            pub fn result(result: w::BuildResult) -> $crate::BuildResult { result.into_native() }
            pub fn support(support: w::TargetSupport) -> $crate::TargetSupport { support.into_native() }
        }
    };
}
