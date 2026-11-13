//! Runtime, foreign and Flick imports for one generated module.

use flick_pretty::Document;

use super::{Gen, RUNTIME, names, relative};

impl<'d> Gen<'_, 'd> {
    /// The import lines of the module at `here`.
    pub(super) fn imports_of(&self, here: &str) -> Vec<Document<'d>> {
        let mut lines = Vec::new();
        let mut line = |text: String| lines.push(self.a.text(self.a.alloc_str(&text)));
        let runtime = (self.options.foreign)(self.module, RUNTIME);
        if self.effect {
            line(format!("import {{ Effect }} from \"{runtime}\";"));
        }
        // Only exports carry TypeScript types; imported module members are any.
        let typescript = self.options.typescript();
        let untyped = |line: &mut dyn FnMut(String), alias: &str| {
            if typescript {
                line(format!("const {alias}: any = {alias}$;"));
            }
        };
        let bound = |alias: &str| {
            if typescript {
                format!("{alias}$")
            } else {
                alias.to_owned()
            }
        };
        if self.runtime {
            line(format!("import * as {} from \"{runtime}\";", bound("$rt")));
            untyped(&mut line, "$rt");
        }
        for (source, export, alias) in &self.foreign {
            let export = if names::is_property(export) {
                export.clone()
            } else {
                names::string(export)
            };
            line(format!(
                "import {{ {export} as {} }} from {};",
                bound(alias),
                names::string(source)
            ));
            untyped(&mut line, alias);
        }
        for imported in &self.imports {
            let target = (self.options.path)(imported);
            let alias = self
                .aliases
                .get(imported)
                .cloned()
                .unwrap_or_else(|| names::module_alias(imported, 'm'));
            let from = names::string(&relative(here, &target));
            line(format!("import * as {} from {from};", bound(&alias)));
            untyped(&mut line, &alias);
        }
        lines
    }
}
