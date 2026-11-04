//! A use of `HttpApi.Derive.wire_codec`, as the codec the checker chose
//! for it: the value named after the type, or `Codec.json` of its schema.

use super::Lower;
use crate::ir::{Expr, ExprKind, Stage};
use crate::source::Span;
use crate::types::wire::WireCodec;
use crate::types::{Range, companion_name};

impl Lower<'_> {
    pub(super) fn wire_codec(&self, key: Range, span: Span) -> ExprKind {
        let global = |module: &str, name: String| ExprKind::Global {
            module: module.to_owned(),
            name,
        };
        match self.typing.wire_codecs.get(&key) {
            Some(WireCodec::Value { module, name }) => global(module, name.clone()),
            Some(WireCodec::Schema { module, ty }) => ExprKind::App {
                func: Box::new(Expr::new(global("Codec", "json".to_owned()), span)),
                arg: Box::new(Expr::new(
                    global(module, companion_name(ty, "schema")),
                    span,
                )),
                stage: Stage::Pure,
                arg_first: false,
            },
            // the checker reported the use
            None => global("HttpApi.Derive", "wire_codec".to_owned()),
        }
    }
}
