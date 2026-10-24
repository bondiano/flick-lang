//! "Qualify" an imported name at every use, or "unqualify" `M.name` and
//! import it by name.

use super::super::{Ide, Symbol, tree};
use super::{Action, edit, to_u32};
use crate::iface::Namespace;
use crate::resolve::{Import, ItemKind, declarations, item_removal, module_syntax};
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Ide<'_> {
    pub(super) fn qualify(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let (Symbol::Entity(entity), (start, _)) = self.symbol_at(file, offset)? else {
            return None;
        };
        if entity.namespace == Namespace::Type && entity.module == m.name {
            return None;
        }
        let syntax = module_syntax(&m.tree, text, file);
        let qualified = start > 0 && text.as_bytes()[start as usize - 1] == b'.';
        // the import that brings the entity
        let brings = |import: &&Import| {
            self.interface(&import.module).is_some_and(|i| {
                i.find(entity.namespace, &entity.name)
                    .is_some_and(|e| e.entity == entity)
            })
        };
        let import = syntax.imports.iter().filter(brings).find(|i| {
            qualified
                || i.exposing
                    .iter()
                    .any(|it| it.name == entity.name && it.kind != ItemKind::All)
        })?;
        let uses: Vec<(u32, u32)> = self
            .references(&Symbol::Entity(entity.clone()))
            .into_iter()
            .filter(|(f, _)| *f == file)
            .map(|(_, r)| r)
            .filter(|r| !in_import(&m.tree, r.0))
            .collect();
        let q = format!("{}.", import.qualifier());
        let prefixed = |s: u32| text[..s as usize].ends_with(&q);
        if qualified {
            self.unqualify(file, import, &entity.name, &uses, &q)
        } else {
            let mut edits: Vec<_> = uses
                .iter()
                .filter(|(s, _)| !prefixed(*s) && !text[..*s as usize].ends_with('.'))
                .map(|&(s, e)| edit(file, s, e, format!("{q}{}", entity.name)))
                .collect();
            let item = import.exposing.iter().find(|it| it.name == entity.name)?;
            let (s, e) = item_removal(text, import, item.span);
            edits.push(edit(file, s, e, ""));
            Some(Action {
                title: format!("qualify `{}` as `{q}{}`", entity.name, entity.name),
                kind: "refactor.rewrite",
                edits,
            })
        }
    }

    /// `Q.name` everywhere becomes `name`, imported by name from `import`.
    fn unqualify(
        &self,
        file: FileId,
        import: &Import,
        name: &str,
        uses: &[(u32, u32)],
        q: &str,
    ) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        // a name the module defines or imports otherwise would change meaning
        let taken = declarations(&m.tree, text, file)
            .iter()
            .any(|d| d.name == name)
            || module_syntax(&m.tree, text, file)
                .imports
                .iter()
                .any(|i| i.exposing.iter().any(|it| it.name == name));
        if taken {
            return None;
        }
        let mut edits: Vec<_> = uses
            .iter()
            .filter(|(s, _)| text[..*s as usize].ends_with(q))
            .map(|&(s, e)| edit(file, s - to_u32(q.len()), e, name))
            .collect();
        let decl = m.tree.nodes().find(|n| {
            n.kind == K::ImportDecl
                && n.range().0 <= import.span.start
                && import.span.end <= n.range().1
        })?;
        edits.push(match import.exposing.last() {
            Some(last) => {
                let close = text[last.span.end as usize..].find(')')?;
                let at = last.span.end + to_u32(close);
                edit(file, at, at, format!(", {name}"))
            }
            None => edit(
                file,
                decl.range().1,
                decl.range().1,
                format!(" exposing ({name})"),
            ),
        });
        Some(Action {
            title: format!("import `{name}` and drop the `{q}` qualifier"),
            kind: "refactor.rewrite",
            edits,
        })
    }
}

fn in_import(root: &Node, offset: u32) -> bool {
    tree::path_to(root, offset)
        .iter()
        .any(|n| matches!(n.kind, K::ImportDecl | K::ModuleHeader))
}
