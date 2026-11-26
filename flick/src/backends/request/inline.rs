//! The inline foreign code of a program: each module's preambles and each
//! foreign item bound to inline code, as a build request carries them.

use flick_backend::{InlineForeign, Span};
use flick_ir::Program;

pub(crate) fn collect_inline_foreign(program: &Program) -> Vec<InlineForeign> {
    let mut out = Vec::new();
    for module in &program.modules {
        for preamble in &module.preambles {
            out.push(InlineForeign {
                module: module.name.clone(),
                name: None,
                code: preamble.code.clone(),
                span: Span {
                    file: preamble.span.file.index(),
                    start: preamble.span.start,
                    end: preamble.span.end,
                },
            });
        }
        for item in &module.items {
            if let flick_ir::ItemKind::Foreign(foreign) = &item.kind
                && let flick_ir::Binding::Inline(inline) = &foreign.binding
            {
                out.push(InlineForeign {
                    module: module.name.clone(),
                    name: Some(foreign.name.clone()),
                    code: inline.code.clone(),
                    span: Span {
                        file: inline.span.file.index(),
                        start: inline.span.start,
                        end: inline.span.end,
                    },
                });
            }
        }
    }
    out
}
