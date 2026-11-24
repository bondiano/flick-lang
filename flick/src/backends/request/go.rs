//! The facts of the go target beyond the IR: each own module's exported
//! values and the path its `//line` directives give, the Go file beside each
//! relative foreign module, direct Go package bindings, declared
//! `[target.go.dependencies]` and optional `go.sum`, and the binding tables
//! of the implicit packages.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use flick_backend::{Abi, Descriptor, ForeignBinding, GoProjection, InputKind, Projection};
use flick_core::build::go::{foreign_files, line_path};
use flick_core::codegen::{EffectBridge, go::is_relative};
use flick_core::iface::{Interface, Namespace};
use flick_core::project::{Compilation, Project};
use flick_core::types;
use flick_ir::{Binding as IrBinding, ItemKind};

use super::Prepared;
use crate::commands::build::BuildPlan;
use crate::commands::build::settings::with_settings;
use crate::sources::FsHost;

/// The prefix of the resource inputs that carry the binding tables.
const BINDINGS: &str = "go/bindings/";
/// Declared `[target.go.dependencies]` entries (`<module> <version>\n`).
const GO_DEPENDENCIES: &str = "go/dependencies";
/// Optional project-level `go.sum` lockfile.
const GO_SUM_INPUT: &str = "go/go.sum";

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
        descriptor: &Descriptor,
    ) {
        let binding_abi = descriptor.value_abi.as_ref().map_or_else(
            || Abi {
                name: "flick.go-ffi".into(),
                version: "0.1.0".into(),
            },
            |v| v.abi.clone(),
        );
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
                        abi: binding_abi.clone(),
                        inputs: vec![input],
                    });
                }
            },
        );
        self.go_package_bindings_and_deps(project, compilation, &binding_abi);
        // two packages may name a table alike
        for (i, (file, text)) in types::implicit_go_bindings().enumerate() {
            let id = format!("{BINDINGS}{i}/{file}");
            self.input(id, InputKind::Resource, text.as_bytes().to_vec());
        }
    }

    fn go_package_bindings_and_deps(
        &mut self,
        project: Option<&Project>,
        compilation: &Compilation,
        binding_abi: &Abi,
    ) {
        // Direct Go package bindings (`from "math" "Hypot"`, etc.) carry the active
        // Go value-ABI identifier for `FLICK-F014` verification without sibling files.
        let mut seen_pkg_bindings = BTreeSet::new();
        for module in &compilation.modules {
            for item in &module.ir.items {
                if let ItemKind::Foreign(foreign) = &item.kind
                    && let IrBinding::Module { from, .. } = &foreign.binding
                    && !is_relative(from)
                    && !from.starts_with("flick:")
                    && seen_pkg_bindings.insert((module.name.as_str(), from.as_str()))
                {
                    self.request.foreign.push(ForeignBinding {
                        module: module.name.clone(),
                        specifier: from.clone(),
                        resolved: from.clone(),
                        abi: binding_abi.clone(),
                        inputs: Vec::new(),
                    });
                }
            }
        }
        // Pass `[target.go.dependencies]` from the project and library closure.
        let mut go_deps = BTreeMap::new();
        for wanted in &compilation.target_dependencies {
            if wanted.target == "go" {
                go_deps.insert(wanted.name.clone(), wanted.version.clone());
            }
        }
        if !go_deps.is_empty() {
            let mut text = String::new();
            for (module, version) in go_deps {
                text.push_str(&module);
                text.push(' ');
                text.push_str(&version);
                text.push('\n');
            }
            self.input(
                GO_DEPENDENCIES.to_owned(),
                InputKind::TargetFacts,
                text.into_bytes(),
            );
        }
        if let Some(project) = project
            && let Ok(bytes) = fs::read(project.root.join("go.sum"))
        {
            self.input(GO_SUM_INPUT.to_owned(), InputKind::Lockfile, bytes);
        }
    }
}
