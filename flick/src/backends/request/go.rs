//! The facts of the go target beyond the IR: each own module's exported
//! values and the path its `//line` directives give, the Go file beside each
//! foreign module, and the binding tables of the implicit packages.

use std::fs;
use std::path::Path;

use flick_backend::{Abi, ForeignBinding, GoProjection, InputKind, Projection};
use flick_core::build::go::{foreign_files, line_path};
use flick_core::codegen::EffectBridge;
use flick_core::iface::{Interface, Namespace};
use flick_core::project::{Compilation, Project};
use flick_core::types;

use super::Prepared;
use crate::commands::build::BuildPlan;
use crate::commands::build::settings::with_settings;
use crate::sources::FsHost;

/// The prefix of the resource inputs that carry the binding tables.
const BINDINGS: &str = "go/bindings/";

/// The values importers of the module of `interface` see.
fn exports(interface: &Interface) -> Vec<String> {
    (interface.exports.iter())
        .filter(|e| e.entity.namespace == Namespace::Value && e.entity.module == interface.module)
        .map(|e| e.entity.name.clone())
        .collect()
}

impl Prepared {
    fn go_facts(&mut self, module: &str, facts: &GoProjection) {
        let mut bytes = Vec::new();
        ciborium::into_writer(facts, &mut bytes).expect("Go facts serialize");
        let input = self.input(format!("go/{module}"), InputKind::TargetFacts, bytes);
        self.request.projections.push(Projection {
            module: module.to_owned(),
            schema: Abi {
                name: "flick.go".into(),
                version: "0.1.0".into(),
            },
            input,
        });
    }

    pub(super) fn go(
        &mut self,
        host: &FsHost,
        project: Option<&Project>,
        compilation: &Compilation,
        output: &BuildPlan,
    ) {
        with_settings(
            host,
            project,
            compilation,
            output,
            EffectBridge::Absent,
            |settings| {
                for module in &compilation.modules {
                    let facts = GoProjection {
                        exports: exports(&module.interface),
                        line_path: line_path(settings, &module.path),
                    };
                    self.go_facts(&module.name, &facts);
                }
                // a release build specializes and inlines from them as from the build's
                for interface in types::builtins_of(false) {
                    if !compilation
                        .modules
                        .iter()
                        .any(|m| m.name == interface.module)
                    {
                        let facts = GoProjection {
                            exports: exports(interface),
                            line_path: String::new(),
                        };
                        self.go_facts(&interface.module, &facts);
                    }
                }
                let read = |path: &Path| fs::read_to_string(path).ok();
                for ((module, specifier), (resolved, text)) in
                    foreign_files(compilation, settings, &read)
                {
                    let input = self.input(
                        format!("foreign/go/{module}/{specifier}"),
                        InputKind::Foreign,
                        text.into_bytes(),
                    );
                    self.request.foreign.push(ForeignBinding {
                        module,
                        specifier,
                        resolved,
                        abi: Abi {
                            name: "flick.go-ffi".into(),
                            version: "0.1.0".into(),
                        },
                        inputs: vec![input],
                    });
                }
            },
        );
        // two packages may name a table alike
        for (i, (file, text)) in types::implicit_go_bindings().enumerate() {
            let id = format!("{BINDINGS}{i}/{file}");
            self.input(id, InputKind::Resource, text.as_bytes().to_vec());
        }
    }
}
