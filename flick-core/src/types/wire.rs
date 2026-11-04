//! `(HttpApi.Derive.wire_codec : PureCodec Json T)`, which `deriving
//! HttpApi` writes for each part and success of an endpoint: the codec a
//! program names after `T` in `T`'s module when it has that type, `T$json`
//! of a refined type, `String.json` of a type of the Prelude, or else the
//! JSON codec of `T`'s schema. Lowering puts it in the use's place.

use std::mem;

use serde::{Deserialize, Serialize};

use super::Checker;
use super::ty::Type;
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang;

/// The codec a use of `wire_codec` stands for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireCodec {
    /// The value named after the type in its module: `update_user_body`.
    Value { module: String, name: String },
    /// `Codec.json` of the type's witness `T$schema`.
    Schema { module: String, ty: String },
}

/// `UpdateUserBody` → `update_user_body`.
pub fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

impl Checker<'_> {
    /// `expr : annotation`, when `expr` is `wire_codec`: the codec of the
    /// type the annotation, `PureCodec Json T`, ends with.
    pub(super) fn wire_codec(&mut self, expr: &Node, annotation: &Node, ty: &Type) {
        // the use is the name, as `HttpApi.Derive.wire_codec`, or its last part
        let (start, end) = expr.range();
        let Some(i) =
            (self.typing.pending_wire.iter()).position(|((s, e), _)| start <= *s && *e <= end)
        else {
            return;
        };
        let (at, _) = self.typing.pending_wire.swap_remove(i);
        let of = match bare(annotation) {
            n if n.kind == K::AppType => n.nodes().last().map(bare),
            _ => None,
        };
        let Some(of) = of else {
            return self.unannotated(self.span(expr));
        };
        match self.codec_of(of, ty) {
            Ok(codec) => {
                self.typing.wire_codecs.insert(at, codec);
            }
            Err(message) => self.error(codes::D011, self.span(of), message),
        }
    }

    /// A `wire_codec` with no type to read its codec from.
    pub(super) fn unannotated(&mut self, span: Span) {
        self.error(
            codes::D011,
            span,
            "`wire_codec` is written `(wire_codec : PureCodec Json T)`, for a type `T`".to_owned(),
        );
    }

    /// The codec of the type written at `node`, as `ty` is its codec's type.
    fn codec_of(&mut self, node: &Node, ty: &Type) -> Result<WireCodec, String> {
        let names: Vec<&str> = (node.child_tokens())
            .filter(|t| t.kind == K::Con)
            .map(|t| t.text(self.text))
            .collect();
        let shown = names.join(".");
        let no_codec = || {
            format!(
                "`{shown}` has no codec: name a `PureCodec Json` of it after it in its module, or derive `Schema`"
            )
        };
        let (true, Some((name, qualifier))) = (node.kind == K::NameType, names.split_last()) else {
            return Err(no_codec());
        };
        let (module, _) = self
            .find_type(node, &qualifier.join("."), name)
            .ok_or_else(no_codec)?;
        let mut candidates = vec![
            (module.clone(), snake(name)),
            (module.clone(), super::companion_name(name, "json")),
        ];
        if module == lang::prelude() {
            candidates.push(((*name).to_owned(), "json".to_owned()));
        }
        // a value of that name and another type, `name : String` for `Name`, is no codec of it
        let found = candidates.into_iter().find(|(module, name)| {
            let entity = Entity {
                module: module.clone(),
                namespace: Namespace::Value,
                name: name.clone(),
            };
            let declared = self.env.value(&entity).cloned();
            declared
                .is_some_and(|t| !matches!(t, Type::Forall { .. }) && self.unify(&t, ty).is_ok())
        });
        if let Some((module, name)) = found {
            return Ok(WireCodec::Value { module, name });
        }
        let schema = Entity {
            module: module.clone(),
            namespace: Namespace::Value,
            name: super::companion_name(name, "schema"),
        };
        if self.env.value(&schema).is_some() {
            return Ok(WireCodec::Schema {
                module,
                ty: (*name).to_owned(),
            });
        }
        Err(no_codec())
    }

    /// The uses of `wire_codec` outside an annotation.
    pub(super) fn unannotated_codecs(&mut self) {
        for (_, span) in mem::take(&mut self.typing.pending_wire) {
            self.unannotated(span);
        }
    }
}

/// `node` without the parentheses around it.
fn bare(node: &Node) -> &Node {
    match (node.kind, node.nodes().next()) {
        (K::ParenType, Some(inner)) => bare(inner),
        _ => node,
    }
}
