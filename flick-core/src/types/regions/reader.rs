//! Function reader regions lent to lazy `for` comprehensions.

use super::qualify;
use crate::diagnostics::{self, codes};
use crate::source::Span;
use crate::types::Checker;
use crate::types::ty::{Row, Type, capture_label};

/// A function's reader: the services it reads atoms through, which end
/// when it returns, so a lazy `for` that reads them cannot leave it.
#[derive(Debug, Clone)]
pub struct Reader {
    pub level: u32,
    /// The region, once a `for` needs it, and that `for`.
    pub region: Option<(u32, Span)>,
}

impl Checker<'_> {
    /// Starts the reader of a function whose body is checked next.
    pub fn begin_reader(&mut self) {
        let level = self.store.level;
        self.cx.regions.readers.push(Reader {
            level,
            region: None,
        });
    }

    /// The region of the reader the innermost function lends to a lazy
    /// `for` at `span`.
    pub fn reader_region(&mut self, span: Span) -> Type {
        let level = self
            .cx
            .regions
            .readers
            .last()
            .map_or(self.store.level, |r| r.level);
        if let Some(Reader {
            region: Some((id, _)),
            ..
        }) = self.cx.regions.readers.last()
        {
            return Type::Skolem(*id);
        }
        let region = self.store.skolem_at("reader", level);
        if let Type::Skolem(id) = &region {
            self.cx.regions.lent.push(*id);
            if let Some(reader) = self.cx.regions.readers.last_mut() {
                reader.region = Some((*id, span));
            }
        }
        region
    }

    /// `ty` qualified with the reader `region` too. An inferred qualifier is
    /// the sequence's own: it already holds what it reads.
    pub fn read_through(&mut self, ty: Type, region: &Type) -> Type {
        let label = capture_label(region).expect("a skolem has a label");
        let held = Type::Row(Row::closed(vec![(label, region.clone())]));
        let inferred = match &ty {
            Type::Captured { captures, .. } => match **captures {
                Type::Meta(ref meta)
                    if meta.inference_type_root().is_some()
                        && matches!(self.store.resolve(captures), Type::Meta(_)) =>
                {
                    meta.inference_type_root()
                }
                _ => None,
            },
            _ => None,
        };
        match inferred {
            Some(m) => {
                self.store.set(m, held);
                ty
            }
            None => qualify(ty, held),
        }
    }

    /// Ends the innermost function's reader, reporting its `result` if it
    /// holds the reader.
    pub fn end_reader(&mut self, result: &Type, span: Span) {
        let Some(Reader {
            region: Some((id, at)),
            ..
        }) = self.cx.regions.readers.pop()
        else {
            return;
        };
        let mut found = false;
        self.zonk(result)
            .visit(&mut |t| found |= *t == Type::Skolem(id));
        if !found {
            return;
        }
        let head = self.show_exact(result);
        self.diagnostics.push(
            diagnostics::Diagnostic::error(
                codes::G002,
                format!(
                    "the result has type `{head}`, a sequence whose clauses read atoms through this function's reader, so it cannot leave the function"
                ),
                span,
            )
            .with_related(at, "the sequence reads atoms here")
            .with_fix(|| ("build a list with `[for …]`, or iterate the sequence in this function", Vec::new())),
        );
    }
}
