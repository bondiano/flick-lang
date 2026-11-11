//! Attributes are compiler metadata, not ordinary resolved names. Explain
//! their contract even in incomplete or otherwise invalid editor buffers.

use std::fmt::Write as _;

use super::{Ide, documentation, tree};
use crate::doc;
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};

const LANG: &str = "Marks a declaration as a lang item: a built-in entity the compiler refers to by its role rather than its module and name. Used in built-in modules only; user modules cannot register lang items. Takes one item name, for example `@lang action`.";
const MAKER: &str = "Marks a value as a maker of an atom, action or view. The compiler tracks calls to it, including through other functions, to reject creating a new one on every render (`FLICK-U006`). Create it once at the top level or during component setup instead. Takes no argument.";
const DERIVE: &str = "Marks a class as derivable with `deriving`. The implementation comes from the plugin of the class's package, or from the package named by the attribute, for example `@derive std`. Belongs before a class declaration.";
const IMPLICIT: &str = "Opts an instance into automatic conversion between two distinct ground nominal types when the expected type is known. The class must have two parameters and one pure method of type `a -> b`; the instance must have no context or type variables. Exactly one visible instance must match; conversions do not chain. Takes no argument and belongs before an instance declaration.";

impl Ide<'_> {
    pub(super) fn attribute_hover(
        &self,
        file: FileId,
        offset: u32,
    ) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let attribute = path.iter().find(|n| n.kind == K::Attribute)?;
        let token = attribute
            .child_tokens()
            .find(|t| t.start <= offset && offset < t.end)?;
        let at = attribute.child_tokens().find(|t| t.kind == K::At)?;
        let mut words = attribute.child_tokens().filter(|t| t.kind == K::Ident);
        let name = words.next()?;
        let argument = words.next();
        let on_name = token.start <= name.start;
        let range = if on_name {
            (at.start, name.end)
        } else {
            (token.start, token.end)
        };
        let description = match name.text(text) {
            "maker" if on_name => MAKER,
            "implicit" if on_name => IMPLICIT,
            "derive" => DERIVE,
            "lang" if on_name || Lang::from_name(token.text(text)).is_some() => LANG,
            _ => return None,
        };
        let mut code = format!("@{}", name.text(text));
        if let Some(argument) = argument {
            let _ = write!(code, " {}", argument.text(text));
        }
        let mut markdown = format!("```flick\n{code}\n```\n\n{description}");
        match name.text(text) {
            "derive" => {
                if let Some(package) = argument {
                    let _ = write!(
                        markdown,
                        "\n\nThe plugin of `{}` supplies this class's `deriving` implementation.",
                        package.text(text),
                    );
                }
            }
            "lang" => {
                if let Some(item) = argument.and_then(|t| Lang::from_name(t.text(text))) {
                    self.lang_item_documentation(item, &mut markdown);
                }
            }
            _ => {}
        }
        Some((markdown, range))
    }

    /// Use the same registry and declaration docs as the checker. New lang
    /// items and library renames automatically get the corresponding hover.
    fn lang_item_documentation(&self, lang: Lang, markdown: &mut String) {
        let Some(item) = lang::table().find(lang) else {
            return;
        };
        let source = self.source(&item.module);
        let module = matches!(lang, Lang::Prelude | Lang::Instances);
        let name = if module {
            item.module.clone()
        } else {
            format!("{}.{}", item.module, item.name)
        };
        let _ = write!(markdown, "\n\nCompiler role `{}`: `{name}`.", lang.name());
        let docs = source.and_then(|source| {
            if module {
                doc::module_comment(&source.text)
            } else {
                documentation(&source, &item.name)
            }
        });
        if let Some(docs) = docs {
            let _ = write!(markdown, "\n\n{docs}");
        }
    }
}
