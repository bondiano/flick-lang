//! The enums a module sees, for `literal-of` of its blocks: those it
//! declares, and those of the modules it imports, by the names it writes
//! them with, `Role` or `Roles.Role`. The types are not checked yet, so the
//! declarations say it: an `untagged type` whose constructors are literals.

use std::collections::BTreeMap;

use super::super::{Loaded, Loader};
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::parse;
use crate::types;

impl Loader<'_> {
    /// The enums `m` sees, each with its base type, `String` or `Int`.
    pub(super) fn enums_seen(&self, m: &Loaded) -> BTreeMap<String, String> {
        let text = &self.sources.get(m.file).text;
        let mut seen: BTreeMap<String, String> =
            declared(&m.parse.tree, text).into_iter().collect();
        for import in &m.syntax.imports {
            let theirs = self.enums_of(&import.module);
            for (name, base) in &theirs {
                seen.insert(format!("{}.{name}", import.qualifier()), base.clone());
                let exposed =
                    (import.exposing.iter()).any(|e| e.name.is_empty() || e.name == *name);
                if exposed {
                    seen.entry(name.clone()).or_insert_with(|| base.clone());
                }
            }
        }
        seen
    }

    /// The enums `module` declares, from the loaded modules or the built-in
    /// ones.
    fn enums_of(&self, module: &str) -> Vec<(String, String)> {
        if let Some(&j) = self.by_name.get(module) {
            let m = &self.modules[j];
            return declared(&m.parse.tree, &self.sources.get(m.file).text);
        }
        (types::builtin_sources().iter())
            .find(|(name, _)| *name == module)
            .map(|(_, text)| declared(&parse(text, FileId::default()).tree, text))
            .unwrap_or_default()
    }
}

/// The enums `tree` declares, each with its base type.
fn declared(tree: &Node, text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for decl in tree.nodes().filter(|n| n.kind == K::TypeDecl) {
        let Some(name) = decl.child_tokens().find(|t| t.kind == K::Con) else {
            continue;
        };
        let literals: Vec<Option<&Node>> = (decl.nodes())
            .filter(|n| n.kind == K::Variant)
            .map(|v| v.nodes().find(|n| n.kind == K::LitPat))
            .collect();
        let Some(Some(first)) = literals.first() else {
            continue;
        };
        if literals.iter().any(Option::is_none) {
            continue;
        }
        let string = first.child_tokens().any(|t| t.kind == K::String);
        let base = if string { "String" } else { "Int" };
        out.push((name.text(text).to_owned(), base.to_owned()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::declared;
    use crate::source::FileId;
    use crate::syntax::parse;

    #[test]
    fn an_untagged_type_of_literals_is_an_enum_of_their_base() {
        let text = "module M exposing (..)\n\
             untagged type Role = Admin = \"admin\" | Guest = \"guest\"\n\
             untagged type Code = Ok200 = 200 | Missing = -404\n\
             untagged type Key = ByName String | ByNumber Float\n\
             type Plain = A | B\n";
        let found = declared(&parse(text, FileId::default()).tree, text);
        let expected =
            [("Role", "String"), ("Code", "Int")].map(|(n, b)| (n.to_owned(), b.to_owned()));
        assert_eq!(found, expected);
    }
}
