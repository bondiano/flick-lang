//! The `import type` lines of a generated module: what the backend writes
//! itself, the exports the forms it wrote name, and the modules whose
//! types it named.

use std::fmt::Write;

use super::{
    EFFECT_RUNTIME, EFFECT_RUNTIME_UI, FLICK_RUNTIME, FLICK_RUNTIME_UI, OWN, Origin, Projection,
    names,
};

impl Projection<'_> {
    /// `import type` lines for what the projection referenced, with
    /// `path(module)` the import specifier of a module.
    pub fn imports(&self, path: &dyn Fn(&str) -> String) -> String {
        let runtime = if self.rt {
            FLICK_RUNTIME
        } else {
            EFFECT_RUNTIME
        };
        let runtime_ui = if self.rt {
            FLICK_RUNTIME_UI
        } else {
            EFFECT_RUNTIME_UI
        };
        // what the backend writes itself: the stages of functions, the
        // results of runners and the tags of services
        let mut out = if self.rt {
            format!(
                "import type {{ EffectValue as $Effect, ResultValue as $Result, ServiceValue as $Service }} from \"{runtime}\";\n"
            )
        } else {
            format!(
                "import type {{ Effect as $Effect, Result as $Result, Context as $Context }} from \"{runtime}\";\n"
            )
        };
        // and what the forms of the types named, each slot on a line
        let imported = self.imported.borrow();
        for (slot, from) in [(Origin::Runtime, runtime), (Origin::Ui, runtime_ui)] {
            let names: Vec<String> = (imported.iter())
                .map(|(i, _)| i)
                .filter(|i| i.from == slot && !OWN.contains(&i.effect.as_str()))
                .map(|i| {
                    let export = if self.rt { &i.rt } else { &i.effect };
                    format!("{export} as ${}", i.effect)
                })
                .collect();
            if !names.is_empty() {
                let _ = writeln!(
                    out,
                    "import type {{ {} }} from \"{from}\";",
                    names.join(", ")
                );
            }
        }
        // and each export of a module a package names: a file is beside
        // the module that names it
        for (import, module) in imported.iter() {
            let Origin::Module(spec) = &import.from else {
                continue;
            };
            let from = if spec.starts_with("./") || spec.starts_with("../") {
                beside(&path(module), &(self.foreign)(module, spec))
            } else {
                spec.clone()
            };
            let export = if self.rt { &import.rt } else { &import.effect };
            let _ = writeln!(
                out,
                "import type {{ {export} as ${}{} }} from {};",
                import.effect,
                Self::alias(module),
                names::string(&from)
            );
        }
        for module in self.used.borrow().iter() {
            let _ = writeln!(
                out,
                "import type * as {} from {};",
                Self::alias(module),
                names::string(&path(module))
            );
        }
        out
    }
}

/// The specifier, from the file that writes `file` for a module, of what
/// that module names `spec` beside itself.
pub(super) fn beside(file: &str, spec: &str) -> String {
    let mut parts: Vec<&str> = file.split('/').collect();
    parts.pop();
    for part in spec.split('/') {
        match part {
            "." => {}
            ".." if parts.last().is_some_and(|p| !p.starts_with('.')) => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}
