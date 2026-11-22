//! Hover details for scope-owned `defer` and `errdefer` statements.

use super::super::Ide;
use crate::source::FileId;

impl Ide<'_> {
    pub(super) fn defer_hover(
        &self,
        file: FileId,
        is_errdefer: bool,
        at: (u32, u32),
    ) -> Option<(String, String, (u32, u32))> {
        let m = self.module(file)?;
        let head = if is_errdefer {
            "errdefer { … } : Unit"
        } else {
            "defer { … } : Unit"
        };
        let mode = if is_errdefer {
            "Runs only when the owning `scope` body fails (escaping typed failure, defect, or cancellation; skipped on success or returned `Err`/`None` data)."
        } else {
            "Runs on every close of its owning `scope` (success, typed failure, defect, or cancellation)."
        };
        let note = if let Some(info) = m.typing.defers.get(&at) {
            let (line, col) = self
                .sources
                .get(info.owner_span.file)
                .utf16_position(info.owner_span.start);
            let owner = match &info.owner_name {
                Some(name) => format!("`scope {name}`"),
                None => "`scope`".to_owned(),
            };
            format!(
                "{mode} Owned by {owner} at line {}, column {}. Registered once per reached execution on the owner's LIFO finalizer stack.",
                line + 1,
                col + 1
            )
        } else {
            format!("{mode} Requires a lexically enclosing explicit `scope {{ … }}`.")
        };
        Some((head.to_owned(), note, at))
    }
}
