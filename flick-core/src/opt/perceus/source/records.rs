//! Audited `Record.*` row-primitive adapters (`get`, `extend`, `remove`, `project`, `merge_disjoint`, `labels`).

use flick_ir::{Foreign, ForeignMode};

use super::lazy_scope::has_standard_shape;
use super::{Code, Intrinsic, Local, Lower, LowerError, Span};

impl Lower<'_> {
    pub(super) fn record_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.binding.from() != Some("flick:rt")
            || value.mode != ForeignMode::Pure
            || !has_standard_shape(value)
        {
            return None;
        }
        match (
            value.binding.export_name().unwrap_or(""),
            value.dicts,
            value.arity,
        ) {
            ("recordGet", 1, 2) => {
                Some(self.curried_intrinsic(Intrinsic::RecordGet, 1, false, block, span))
            }
            ("recordExtend", 2, 3) => {
                Some(self.curried_intrinsic(Intrinsic::RecordExtend, 2, false, block, span))
            }
            ("recordRemove", 1, 2) => {
                Some(self.curried_intrinsic(Intrinsic::RecordRemove, 1, false, block, span))
            }
            // `Project r out` supplies `Dict::Primitive { labels }` as dict 0.
            ("recordProject", 1, 1) => {
                Some(self.curried_intrinsic(Intrinsic::RecordProject, 1, true, block, span))
            }
            ("recordMergeDisjoint", 1, 2) => {
                Some(self.curried_intrinsic(Intrinsic::RecordMergeDisjoint, 1, false, block, span))
            }
            ("recordLabels", 1, 1) => {
                Some(self.curried_intrinsic(Intrinsic::RecordLabels, 1, false, block, span))
            }
            _ => None,
        }
    }
}
