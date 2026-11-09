//! Runtime, foreign and Flick imports for one generated module.

use std::fmt::Write;

use super::{Gen, RUNTIME, names, relative};

const KERNEL_RUNTIME: &str = "@flick-lang/runtime";

impl Gen<'_> {
    pub(super) fn write_imports(&self, code: &mut String, here: &str) {
        let runtime = (self.options.foreign)(self.module, RUNTIME);
        if self.effect {
            let _ = writeln!(code, "import {{ Effect }} from \"{runtime}\";");
        }
        // Only exports carry TypeScript types; imported module members are any.
        let untyped = |code: &mut String, alias: &str| {
            if self.options.typescript() {
                let _ = writeln!(code, "const {alias}: any = {alias}$;");
            }
        };
        let bound = |alias: &str| {
            if self.options.typescript() {
                format!("{alias}$")
            } else {
                alias.to_owned()
            }
        };
        if self.runtime {
            let _ = writeln!(code, "import * as {} from \"{runtime}\";", bound("$rt"));
            untyped(code, "$rt");
        }
        // on the kernel, a binding of the runtime it has no export for fails when
        // it is called, not when the module loads: a module binds more than a program uses
        let mut namespaces: Vec<&str> = Vec::new();
        for (source, export, alias) in &self.foreign {
            // a module the kernel does not have is no import at all
            if self.kernel && source.starts_with(&format!("{RUNTIME}/effect/")) {
                let _ = writeln!(
                    code,
                    "const {alias} = $rt.absent({}, {});",
                    names::string(source),
                    names::string(export)
                );
                continue;
            }
            if self.kernel && source.starts_with(KERNEL_RUNTIME) {
                let at = namespaces
                    .iter()
                    .position(|s| s == source)
                    .unwrap_or_else(|| {
                        namespaces.push(source);
                        let _ = writeln!(
                            code,
                            "import * as $n{} from {};",
                            namespaces.len() - 1,
                            names::string(source)
                        );
                        namespaces.len() - 1
                    });
                let member = if names::is_property(export) {
                    format!(".{export}")
                } else {
                    format!("[{}]", names::string(export))
                };
                let _ = writeln!(code, "const {alias} = $n{at}{member};");
                continue;
            }
            let export = if names::is_property(export) {
                export.clone()
            } else {
                names::string(export)
            };
            let _ = writeln!(
                code,
                "import {{ {export} as {} }} from {};",
                bound(alias),
                names::string(source)
            );
            untyped(code, alias);
        }
        for imported in &self.imports {
            let target = (self.options.path)(imported);
            let alias = self
                .aliases
                .get(imported)
                .cloned()
                .unwrap_or_else(|| names::module_alias(imported, 'm'));
            let from = names::string(&relative(here, &target));
            let _ = writeln!(code, "import * as {} from {from};", bound(&alias));
            untyped(code, &alias);
        }
    }
}
