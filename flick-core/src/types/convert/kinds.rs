//! Kinds as written in type parameters: `Type`, `Type -> Type`, `Row Type`,
//! the built-in row kinds and the finite kinds of `kind` declarations.

use super::is_type;
use crate::diagnostics::codes;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::env::Form;
use crate::types::kinds::Kind;

/// `a Type`, `an ErrorRow`.
pub(super) fn with_article(kind: &Kind) -> String {
    let text = kind.to_string();
    let vowel = text.starts_with(['A', 'E', 'I', 'O', 'U']);
    format!("{} {text}", if vowel { "an" } else { "a" })
}

impl Checker<'_> {
    /// A kind written in a type parameter or a `forall`-free position.
    pub fn kind(&mut self, node: &Node) -> Kind {
        let children: Vec<&Node> = node.nodes().collect();
        match node.kind {
            K::ParenType => self.kind(children[0]),
            K::ArrowType => {
                let result = children.iter().skip(1).find(|c| is_type(c.kind));
                let result = result.map_or(Kind::Type, |r| self.kind(r));
                Kind::arrow(self.kind(children[0]), result)
            }
            K::AppType
                if children.len() == 2
                    && children[0].kind == K::NameType
                    && self.text_of(children[0]) == "Row" =>
            {
                Kind::Row(Box::new(self.kind(children[1])))
            }
            K::NameType => {
                let tokens: Vec<_> = node.child_tokens().filter(|t| t.kind != K::Dot).collect();
                let parts: Vec<&str> = tokens.iter().map(|t| t.text(self.text)).collect();
                if let [name] = parts[..]
                    && let Some(kind) = Kind::builtin(name)
                {
                    return kind;
                }
                let Some((name, qualifier)) = parts.split_last() else {
                    return Kind::Type;
                };
                match self.find_type(node, &qualifier.join("."), name) {
                    Some((module, def)) if def.form == Form::Kind => {
                        self.typing.type_names.insert(
                            (node.range().0, node.range().1),
                            (module, (*name).to_owned()),
                        );
                        def.kind
                    }
                    Some(_) => {
                        self.error(
                            codes::T002,
                            self.span(node),
                            format!("`{name}` is a type, not a kind"),
                        );
                        Kind::Type
                    }
                    None => Kind::Type,
                }
            }
            _ => {
                let text = self.text_of(node);
                self.error(
                    codes::T002,
                    self.span(node),
                    format!("`{text}` is not a kind"),
                );
                Kind::Type
            }
        }
    }
}
