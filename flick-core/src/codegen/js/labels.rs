//! Labels of top-level atoms, views and actions: `$rt.label("Module.name",
//! value)` gives each the name debugging tools show, a name nobody writes
//! by hand. Debug labels every one; release what `[build] labels` asks for.

use super::{Gen, names};
use crate::codegen::Labels;
#[cfg(feature = "native-project")]
use crate::source::Span;

impl Gen<'_> {
    /// `value`, the top-level value `name`, with its label if it gets one.
    pub(super) fn labelled(&mut self, name: &str, value: String) -> String {
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
        let text = names::string(&format!("{}.{name}", self.module));
        format!("{}({text}, {value})", self.rt("label"))
    }

    #[cfg(feature = "native-project")]
    /// `code` defining the top-level value `name` as `id`, its source map
    /// naming `id` by `name` where it is defined.
    pub(super) fn named(&mut self, code: String, id: &str, name: &str, span: Span) -> String {
        let defined = format!("const {id}");
        let Some(at) = code.find(&defined) else {
            return code;
        };
        let mark = mark!(self, span);
        self.names.insert(self.spans.len() - 1, name.to_owned());
        let at = at + "const ".len();
        format!("{}{mark}{}", &code[..at], &code[at..])
    }
}
