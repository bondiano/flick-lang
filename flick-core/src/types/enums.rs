//! Enums: an `untagged type` whose constructors are literals,
//! `untagged type Method = Get = "GET" | Post = "POST"`. Each value is its
//! literal at run time; the literals are all strings or all `Int`s, and
//! differ.

use std::mem;

use super::Checker;
use super::env::{Abi, EnumLit, Form};
use crate::diagnostics::codes;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal::{bigint_digits, bigint_digits_value, string_value};

impl Form {
    /// A nominal type of no literals.
    pub const NOMINAL: Form = Form::Nominal {
        literals: Vec::new(),
        hidden: false,
    };
}

/// The literal written after a constructor's `=`, if it has one.
pub(super) fn literal_of(variant: &Node) -> Option<&Node> {
    variant.nodes().find(|n| n.kind == K::LitPat)
}

/// Whether `field` is a string where a constructor's field is: `Get "GET"`,
/// a literal without its `=`.
pub(super) fn literal_as_field(field: &Node, text: &str) -> bool {
    field.kind == K::NameType
        && (field.child_tokens()).any(|t| t.kind == K::String && !t.text(text).is_empty())
}

impl Checker<'_> {
    /// Records the representation of `decl`, of type `name`, when its
    /// constructors are literals, or reports why they cannot be; whether
    /// they are, or one is a literal written as a field, reported already.
    pub(super) fn enum_literals(&mut self, decl: &Node, name: &str) -> bool {
        let variants: Vec<&Node> = decl.nodes().filter(|n| n.kind == K::Variant).collect();
        let Some(literal) = variants.iter().find_map(|v| literal_of(v)) else {
            return variants
                .iter()
                .any(|v| v.nodes().any(|f| literal_as_field(f, self.text)));
        };
        let written = (decl.child_tokens())
            .take_while(|t| t.kind != K::TypeKw)
            .find(|t| t.kind == K::Ident)
            .is_some_and(|t| t.text(self.text) == "untagged");
        if decl.kind != K::TypeDecl {
            let message = "only the constructors of an enum, an `untagged type`, are literals";
            self.error(codes::T020, self.span(literal), message.to_owned());
        } else if !written {
            let message = format!(
                "a literal is the value of a constructor of an enum: write `untagged type {name}`"
            );
            self.error(codes::T020, self.span(literal), message);
        } else if let Some(abi) = self.enum_abi(decl, name) {
            self.record_abi(name, abi);
        }
        true
    }

    /// Reports `Get "GET"`, a literal written as a field of `variant`;
    /// whether `field` is one.
    pub(super) fn literal_field(&mut self, variant: &Node, field: &Node) -> bool {
        if !literal_as_field(field, self.text) {
            return false;
        }
        let con = self.con_token(variant).unwrap_or_default();
        let literal = field.text(self.text).trim().to_owned();
        let message = format!(
            "a constructor's literal comes after `=`: write `{con} = {literal}` in an `untagged type`"
        );
        self.error(codes::T020, self.span(field), message);
        true
    }

    /// The representation of the enum `decl`, of type `name`, each of whose
    /// constructors is a literal; `None` after a diagnostic.
    pub(super) fn enum_abi(&mut self, decl: &Node, name: &str) -> Option<Abi> {
        if decl.nodes().any(|n| n.kind == K::TypeParam) {
            let message = format!(
                "an enum has no type parameters: the constructors of `{name}` hold nothing to use them"
            );
            self.error(codes::T020, self.span(decl), message);
            return None;
        }
        let mut out: Vec<(String, EnumLit)> = Vec::new();
        for variant in decl.nodes().filter(|n| n.kind == K::Variant) {
            let con = self.con_token(variant).unwrap_or_default();
            // a literal written as a field is reported where the field is read
            if variant.nodes().any(|f| literal_as_field(f, self.text)) {
                return None;
            }
            let Some(literal) = literal_of(variant) else {
                let message = format!(
                    "`{con}` has no literal: each constructor of an enum is one, as `{con} = \"…\"`"
                );
                self.error(codes::T020, self.span(variant), message);
                return None;
            };
            if !self.constructor_fields(&con).is_empty() {
                let message = format!("`{con}` of an enum has no fields: its value is its literal");
                self.error(codes::T020, self.span(variant), message);
                return None;
            }
            let value = self.enum_literal(literal)?;
            if let Some((other, _)) = out.iter().find(|(_, l)| *l == value) {
                let message = format!(
                    "`{other}` and `{con}` are the same literal: an enum's literals differ"
                );
                self.error(codes::T020, self.span(literal), message);
                return None;
            }
            if let Some((first, l)) = out.first()
                && mem::discriminant(l) != mem::discriminant(&value)
            {
                let message = format!(
                    "`{first}` is {} and `{con}` {}: an enum's literals are all strings or all `Int`s",
                    kind(l),
                    kind(&value)
                );
                self.error(codes::T020, self.span(literal), message);
                return None;
            }
            out.push((con, value));
        }
        Some(Abi::Literal(out.into_iter().map(|(_, l)| l).collect()))
    }

    /// The value of a constructor's literal: a string, or an integer that
    /// fits in `Int`.
    fn enum_literal(&mut self, literal: &Node) -> Option<EnumLit> {
        let tokens: Vec<_> = literal.child_tokens().collect();
        let negative = tokens.first().is_some_and(|t| t.kind == K::Minus);
        let token = tokens
            .iter()
            .find(|t| matches!(t.kind, K::String | K::Number))?;
        let text = token.text(self.text);
        if token.kind == K::String {
            return Some(EnumLit::String(string_value(text)));
        }
        let shown = if negative {
            format!("-{text}")
        } else {
            text.to_owned()
        };
        let Some(magnitude) = bigint_digits_value(&bigint_digits(text)) else {
            let message =
                format!("`{shown}` is no `Int`: an enum's literal is a string or an `Int`");
            self.error(codes::T020, self.span(literal), message);
            return None;
        };
        let value = if negative { -magnitude } else { magnitude };
        let Ok(value) = i32::try_from(value) else {
            let message =
                format!("`{shown}` does not fit in `Int`, from -2147483648 to 2147483647");
            self.error(codes::T003, self.span(literal), message);
            return None;
        };
        Some(EnumLit::Int(value))
    }
}

fn kind(literal: &EnumLit) -> &'static str {
    match literal {
        EnumLit::String(_) => "a string",
        EnumLit::Int(_) => "an `Int`",
    }
}
