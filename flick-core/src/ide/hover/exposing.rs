//! Hover for `Type(..)` exposing lists in `import` and `module` headers.

use std::fmt::Write;

use super::documentation;
use crate::ide::{Ide, tree};
use crate::iface::{Decl, Namespace};
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::show_signature;

impl Ide<'_> {
    pub(super) fn exposing_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let item = path.iter().find(|n| n.kind == K::ExposeItem)?;
        let dots = item
            .child_tokens()
            .find(|t| t.kind == K::DotDot && t.start <= offset && offset < t.end)?;
        let name = tree::name_of(item, K::Con, text)?;
        let declaration = path
            .iter()
            .find(|n| matches!(n.kind, K::ImportDecl | K::ModuleHeader))?;
        let module: String = declaration
            .nodes()
            .find(|n| n.kind == K::ModulePath)?
            .text(text)
            .split_whitespace()
            .collect();
        let interface = self.interface(&module)?;
        let parent = interface.find(Namespace::Type, name)?;
        let members = match parent.decl {
            Decl::Class => "methods",
            Decl::Kind => "kind members",
            Decl::Adt | Decl::Error => "constructors",
            _ => return None,
        };
        let direction = if declaration.kind == K::ImportDecl {
            "imported"
        } else {
            "exported"
        };
        let mut markdown = format!("**`{name}(..)` — {direction} {members}**\n\nfrom `{module}`");
        let source = self.source(&parent.entity.module);
        let mut any = false;
        for member in interface.members(name) {
            any = true;
            let name = &member.entity.name;
            let code = match (&member.scheme, &member.definition) {
                (Some(scheme), _) => format!("{name} : {}", show_signature(scheme)),
                (_, Some(definition)) => format!("{name} : {}", definition.kind),
                _ => name.clone(),
            };
            let _ = write!(markdown, "\n\n```flick\n{code}\n```");
            if let Some(doc) = source.as_ref().and_then(|s| documentation(s, name)) {
                let _ = write!(markdown, "\n\n{doc}");
            }
        }
        if !any {
            markdown.push_str("\n\nNo public members.");
        }
        Some((markdown, (dots.start, dots.end)))
    }
}
