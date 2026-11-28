//! Contract holes can be solved by value bodies, after nominal snapshots
//! have been completed. Publication must use those final solutions.

use std::io;
use std::path::Path;

use flick_core::diagnostics::{Diagnostic, Severity};
use flick_core::iface::{self, Interface, Namespace};
use flick_core::project::{self, Host};
use flick_core::source::SourceMap;
use flick_core::types::{self, Form, Kind, ModuleRequirement, ModuleSignature, Type, TypeDef};
use flick_core::{resolve, syntax};

const CONTRACTS: &str = r"opaque type Secret = Secret Int
type module S =
  type Box a = List a
  tag : Secret
  value : _
type module Nested =
  module Child : S
type module F = S -> Nested
";

struct Source<'a>(&'a str);

impl Host for Source<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if path == Path::new("/Probe.fl") {
            Ok(self.0.as_bytes().to_vec())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

fn definition<'a>(interface: &'a Interface, name: &str) -> &'a TypeDef {
    interface
        .find(Namespace::Type, name)
        .and_then(|export| export.definition.as_ref())
        .or_else(|| {
            interface
                .support_types
                .iter()
                .find(|support| support.entity.name == name)
                .map(|support| &support.definition)
        })
        .expect("contract or its private support")
}

fn assert_solved(requirements: &[ModuleRequirement]) {
    for requirement in requirements {
        match requirement {
            ModuleRequirement::Value { name, ty } if name == "value" => {
                assert_eq!(*ty, Type::con("Prelude", "Int"));
            }
            ModuleRequirement::Module { signature, .. } => assert_signature(signature),
            _ => {}
        }
    }
}

fn assert_signature(signature: &ModuleSignature) {
    match signature {
        ModuleSignature::Structure { requirements } => assert_solved(requirements),
        ModuleSignature::Parameterized {
            parameter, result, ..
        } => {
            assert_signature(parameter);
            assert_signature(result);
        }
    }
}

fn consumer(interface: &Interface, text: &str) -> Vec<Diagnostic> {
    let mut sources = SourceMap::default();
    let file = sources.add("Consumer.fl", text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let module = resolve::Module {
        name: "Consumer",
        file,
        text,
        tree: &parsed.tree,
        syntax: &syntax,
        generated: resolve::NO_GENERATED,
    };
    // Only the restored interface survives: no producer source or checker.
    let imported = |name: &str| (name == "Probe").then(|| interface.clone());
    let (mut checked, mut diagnostics) = resolve::resolve(&module, &imported);
    diagnostics.extend(types::check(
        &module,
        &imported,
        types::builtins(),
        types::Options {
            local: &|name| name == "Consumer",
            limits: types::Limits::default(),
            no_let_assert: false,
            thresholds: types::Thresholds::default(),
            watched: types::Watched::none(),
            stages_by_class: false,
            foreign_backend: types::ForeignBackendSpec::default_js(),
            validate_foreign_metadata: true,
            native_overrides: None,
            plugin_attributes: &[],
        },
        &mut checked,
    ));
    diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect()
}

#[test]
fn late_solutions_reach_public_private_nested_and_parameterized_contracts() {
    for public in [false, true] {
        let exposed = if public { "S, Nested, F, " } else { "" };
        let text = format!(
            "module Probe exposing ({exposed}Packed, inspect)\n{CONTRACTS}\
                 type alias Packed = exists K : S. Unit\n\
                 inspect : Module S -> Int\n\
                 let inspect packed = unpack packed as K in K.value\n"
        );
        let (diagnostics, compilation) = project::compile(
            &Source(&text),
            &mut SourceMap::default(),
            None,
            &["/Probe.fl".into()],
        );
        assert!(
            diagnostics.iter().all(|d| d.severity != Severity::Error),
            "public={public}: {diagnostics:?}"
        );
        let module = compilation
            .modules
            .iter()
            .find(|module| module.name == "Probe")
            .expect("checked provider");
        for (interface, declarations) in [
            (&module.interface, false),
            (module.declarations.as_ref(), true),
        ] {
            let Form::ModuleContract { requirements } = &definition(interface, "S").form else {
                panic!("S remains a structural contract")
            };
            assert_solved(requirements);
            let ModuleRequirement::Type {
                params,
                manifest: Some(manifest),
                ..
            } = &requirements[0]
            else {
                panic!("parameterized manifest remains present")
            };
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].kind, Kind::Type);
            assert_eq!(
                *manifest,
                Type::app(Type::con("Prelude", "List"), Type::Var("a".into()))
            );
            if public || declarations {
                let Form::ModuleSignature { signature } = &definition(interface, "F").form else {
                    panic!("F remains parameterized")
                };
                assert_signature(signature);
            }
            assert_eq!(
                iface::read(
                    iface::write(interface)
                        .expect("publishable interface")
                        .as_bytes(),
                    "Probe"
                ),
                Ok(interface.clone())
            );
        }
        let restored = iface::read(
            iface::write(&module.interface)
                .expect("publishable interface")
                .as_bytes(),
            "Probe",
        )
        .expect("published interface has no live inference variables");
        assert_eq!(definition(&restored, "Secret").form, Form::NOMINAL);
        assert!(restored.find(Namespace::Constructor, "Secret").is_none());
        let diagnostics = consumer(
            &restored,
            "module Consumer exposing (Again)\nimport Probe\ntype alias Again = Probe.Packed\n",
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        if !public {
            assert!(restored.find(Namespace::Type, "S").is_none());
            let diagnostics = consumer(
                &restored,
                "module Consumer exposing (Leak)\nimport Probe\ntype alias Leak = Module Probe.S\n",
            );
            assert!(
                !diagnostics.is_empty(),
                "private support is not a source export"
            );
        }
    }
}
