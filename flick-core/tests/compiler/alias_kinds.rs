//! Completed alias bodies retain resolved kinds, including unused forall
//! binders, in both published and representation-open declaration views.

use std::io;
use std::path::Path;

use flick_core::diagnostics::Severity;
use flick_core::iface::{self, Namespace};
use flick_core::project::{self, Host};
use flick_core::source::SourceMap;
use flick_core::types::{Form, Kind, Type};

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

#[test]
fn unused_forall_alias_kinds_are_defaulted_before_publication() {
    for opacity in ["", "opaque "] {
        for (body, expected) in [
            ("forall unused. Int", Kind::Type),
            ("forall f. f Int", Kind::arrow(Kind::Type, Kind::Type)),
            (
                "forall c. At c",
                Kind::Named {
                    module: "Probe".into(),
                    name: "Choice".into(),
                },
            ),
        ] {
            let text = format!(
                "module Probe exposing (Poly)\nkind Choice = First | Second\ntype At (c : Choice)\n{opacity}type alias Poly = {body}\n"
            );
            let (diagnostics, compilation) = project::compile(
                &Source(&text),
                &mut SourceMap::default(),
                None,
                &["/Probe.fl".into()],
            );
            assert!(
                diagnostics.iter().all(|d| d.severity != Severity::Error),
                "{diagnostics:?}"
            );
            let module = compilation
                .modules
                .iter()
                .find(|module| module.name == "Probe")
                .expect("checked source");
            let declaration = module
                .declarations
                .find(Namespace::Type, "Poly")
                .and_then(|export| export.definition.as_ref())
                .expect("representation-open alias definition");
            let Form::Alias { body, .. } = &declaration.form else {
                panic!("declaration retains the alias representation")
            };
            let Type::Forall { vars, .. } = body else {
                panic!("written quantification remains present")
            };
            assert_eq!(vars.len(), 1);
            assert_eq!(vars[0].kind, expected, "{opacity}alias: {body:?}");
            for interface in [&module.interface, module.declarations.as_ref()] {
                let encoded = iface::write(interface);
                assert_eq!(
                    iface::read(encoded.as_bytes(), "Probe"),
                    Ok(interface.clone())
                );
            }
        }
    }
}
