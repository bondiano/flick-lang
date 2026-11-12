//! One derive of std's plugin for a module checked on its own: where a class
//! is looked up, and the call that gives its code.

use flick_plugin::{DeriveTarget, Place};

use super::{builtin_derived, made_or};
use crate::diagnostics::{Diagnostic, codes};
use crate::plugin::expansion::Expanded;
use crate::plugin::first_party;
use crate::plugin::render::render;
use crate::plugin::source::BlockSource;
use crate::resolve::ModuleSyntax;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::implicit_package_of;

/// What the derives of a module share: where a class is looked up, and
/// every type the module declares.
pub(super) struct Deriving<'a> {
    pub module: &'a str,
    pub syntax: &'a ModuleSyntax,
    pub summary: &'a [DeriveTarget],
}

impl Deriving<'_> {
    /// The module that declares the class `qualifier.name` with `@derive`
    /// and the package whose plugin derives it, as the loader finds it: the
    /// module, an import exposing it, the Prelude.
    fn owner(&self, qualifier: &str, name: &str) -> Option<(String, String)> {
        // `@derive` alone: the plugin of the class's package
        let marked = |m: &str| {
            (builtin_derived(m).iter())
                .find(|d| d.class == name)
                .map(|d| {
                    let own = implicit_package_of(m).map(str::to_owned);
                    (m.to_owned(), d.package.clone().or(own))
                })
        };
        let imports = &self.syntax.imports;
        let found = if qualifier.is_empty() {
            marked(self.module)
                .or_else(|| {
                    (imports.iter())
                        .filter(|i| {
                            (i.exposing.iter()).any(|e| e.name == name || e.name.is_empty())
                        })
                        .find_map(|i| marked(&i.module))
                })
                .or_else(|| marked("Prelude"))
        } else {
            let import = imports.iter().find(|i| i.qualifier() == qualifier);
            marked(import.map_or(qualifier, |i| i.module.as_str()))
        };
        match found {
            Some((owner, Some(package))) => Some((owner, package)),
            _ => None,
        }
    }

    /// The code the derive of the class `of` gives for `target`, declared
    /// by `node` as `source` reads it; `None` for a class std's plugin does
    /// not derive, or after a diagnostic at `whole`.
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &self,
        node: &Node,
        target: &DeriveTarget,
        mut source: BlockSource,
        of: (&str, &str),
        whole: (u32, u32),
        keep: bool,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<Expanded> {
        let (owner, package) = self.owner(of.0, of.1)?;
        let plugin = first_party::plugin(&package)?;
        let full = format!("{owner}.{}", of.1);
        let at = Span::new(source.file(), whole.0, whole.1);
        let made = plugin.derive(target, &full, None, self.summary, &[], &mut source);
        let arena = made_or(made, &package, &source, at, diagnostics)?;
        let len = source.end() - source.start();
        match render(&arena, len, Place::Declaration, &self.syntax.aliases()) {
            Ok(rendered) => Some(Expanded {
                node: node.clone(),
                rendered,
                source,
                plugin: package,
                keep,
                whole,
            }),
            Err(why) => {
                diagnostics.push(Diagnostic::error(codes::P036, why, at));
                None
            }
        }
    }
}
