//! Labels of top-level atoms, views and actions: `$rt.label("Module.name",
//! value)` gives each the name debugging tools show, a name nobody writes
//! by hand. Debug labels every one; release what `[build] labels` asks for.

use super::Gen;
use super::doc::{Js, call};
use crate::codegen::Labels;

impl<'d> Gen<'_, 'd> {
    /// `value`, the top-level value `name`, with its label if it gets one.
    pub(super) fn labelled(&mut self, name: &str, value: Js<'d>) -> Js<'d> {
        let Some(label) = self.labels.get(name) else {
            return value;
        };
        let kept = match self.options.labels {
            _ if !self.options.release => true,
            Labels::All => true,
            Labels::Components => label.kind == "component",
            Labels::None => false,
        };
        if !kept {
            return value;
        }
        let text = self.string(&format!("{}.{name}", self.module));
        let label = self.rt("label");
        call(self.a, label, &[text, value])
    }
}
