//! References and renaming of the project's own symbols. A rename edits
//! only `.fl` sources: names that generated or foreign code reaches by other
//! spellings (companions, service operations, built-in and package names)
//! have no exact mapping and are refused.

use super::{Ide, Symbol, tree};
use crate::iface::Namespace;
use crate::resolve::declarations;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

mod methods;

/// A name in a file: its UTF-8 byte range.
pub type Occurrence = (FileId, (u32, u32));

impl Ide<'_> {
    /// Every occurrence of `symbol` in the analysis, its declaration first.
    pub fn references(&self, symbol: &Symbol) -> Vec<Occurrence> {
        let mut out = Vec::new();
        if let Symbol::Entity(e) = symbol
            && let Ok(Some(class)) = self.method_class(e)
        {
            out.extend(self.method_references(e, &class));
        }
        if let Some(d) = self.definition(symbol)
            && let super::Target::File(file) = d.target
        {
            out.push((file, (d.start, d.end)));
        }
        for m in &self.analysis.modules {
            let text = &self.sources.get(m.file).text;
            for range in m.typing.names.keys() {
                if let Some(node) = tree::find(&m.tree, *range)
                    && let Some(token) = tree::last_name(node)
                    && self
                        .symbol_at(m.file, token.start)
                        .is_some_and(|(s, _)| s == *symbol)
                {
                    out.push((m.file, (token.start, token.end)));
                }
            }
            for (range, (module, name)) in &m.typing.type_names {
                if let Symbol::Entity(e) = symbol
                    && e.namespace == Namespace::Type
                    && e.module == *module
                    && e.name == *name
                {
                    let at = tree::find(&m.tree, *range)
                        .and_then(tree::last_name)
                        .map_or(*range, |t| (t.start, t.end));
                    out.push((m.file, at));
                }
            }
            for d in declarations(&m.tree, text, m.file) {
                if matches!(symbol, Symbol::Entity(e) if e.module == m.name && e.namespace == d.namespace && e.name == d.name)
                {
                    out.push((m.file, (d.span.start, d.span.end)));
                }
            }
            exposed(&m.tree, &mut |at| {
                if self
                    .symbol_at(m.file, at.0)
                    .is_some_and(|(s, _)| s == *symbol)
                {
                    out.push((m.file, at));
                }
            });
        }
        if let Symbol::Local { file, binder, .. } = symbol {
            out.retain(|(f, _)| f == file);
            out.push((*file, *binder));
        }
        out.sort();
        out.dedup();
        out
    }

    /// The symbol at `offset` if the project may rename it, the range of
    /// its name there and whether names of its kind are capitalized.
    pub fn renamable(
        &self,
        file: FileId,
        offset: u32,
    ) -> Result<(Symbol, (u32, u32), bool), String> {
        let (symbol, range) = self
            .method_at(file, offset)?
            .or_else(|| self.symbol_at(file, offset))
            .ok_or("nothing to rename here")?;
        let capital = match &symbol {
            Symbol::Local { .. } => false,
            Symbol::Entity(e) => {
                let Some(module) = self.analysis.module(&e.module) else {
                    return Err(format!("`{}` is declared outside the project", e.name));
                };
                if let Some(expansion) = &module.expansion
                    && let Some(d) = (expansion.declarations().into_iter())
                        .find(|d| d.namespace == e.namespace && d.name == e.name)
                {
                    let plugin = expansion.plugin_at(d.span).unwrap_or("a plugin");
                    return Err(format!(
                        "`{}` is declared by plugin `{plugin}`: rename it in its block",
                        e.name.replace('$', ".")
                    ));
                }
                if let Some(class) = self.method_class(e)? {
                    self.check_method_mapping(e, &class)?;
                }
                e.namespace != Namespace::Value
            }
            Symbol::Module(_) => return Err("module names require renaming their files".into()),
            Symbol::Operation { .. } | Symbol::InstanceMethod { .. } => {
                return Err("generated code names this by its spelling: no exact mapping".into());
            }
        };
        Ok((symbol, range, capital))
    }

    /// The edits renaming the symbol at `offset` to `new_name`, or why not.
    pub fn rename(
        &self,
        file: FileId,
        offset: u32,
        new_name: &str,
    ) -> Result<Vec<Occurrence>, String> {
        let (symbol, _, capital) = self.renamable(file, offset)?;
        let valid = new_name.chars().next().is_some_and(|c| {
            if capital {
                c.is_ascii_uppercase()
            } else {
                c.is_ascii_lowercase() || c == '_'
            }
        }) && new_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            return Err(format!(
                "`{new_name}` is not a {} name",
                if capital { "capitalized" } else { "lowercase" }
            ));
        }
        self.rename_references(&symbol, new_name)
    }

    /// Rename occurrences of a symbol already accepted by `renamable`.
    /// Workspace callers repeat the mapping and collision checks in each
    /// dependent analysis before returning any edits.
    pub fn rename_references(
        &self,
        symbol: &Symbol,
        new_name: &str,
    ) -> Result<Vec<Occurrence>, String> {
        let edits = self.references(symbol);
        if let Symbol::Entity(e) = symbol
            && let Some(class) = self.method_class(e)?
        {
            self.check_method_mapping(e, &class)?;
            self.check_method_name(e, new_name, &edits)?;
        }
        // every edit must replace the old name exactly
        let old = match symbol {
            Symbol::Local { name, .. } => name.as_str(),
            Symbol::Entity(e) => e.name.as_str(),
            _ => return Err("this symbol has no exact rename mapping".into()),
        };
        for (f, (s, e)) in &edits {
            let text = &self.sources.get(*f).text;
            if text.get(*s as usize..*e as usize) != Some(old) {
                return Err("an occurrence has no exact mapping to the name".into());
            }
        }
        Ok(edits)
    }
}

/// The ranges of names in `exposing` lists of the header and the imports.
fn exposed(root: &Node, f: &mut dyn FnMut((u32, u32))) {
    for decl in root
        .nodes()
        .filter(|n| matches!(n.kind, K::ModuleHeader | K::ImportDecl))
    {
        for exposing in decl.nodes().filter(|n| n.kind == K::Exposing) {
            for item in exposing.nodes() {
                if let Some(t) = item
                    .child_tokens()
                    .find(|t| matches!(t.kind, K::Ident | K::Con))
                {
                    f((t.start, t.end));
                }
            }
        }
    }
}
