// wasm32 stub for the bridge: the browser compiler only emits Effect.
use flick_ir::Shape;

use super::Gen;
use crate::ir::Foreign;

pub(super) enum Toward {
    Host,
    Flick,
}

impl Gen<'_> {
    pub(super) fn bridged_body(
        &mut self,
        _foreign: &Foreign,
        _runtime: bool,
        _call: &str,
        _copy_result: bool,
        _catch: Option<&str>,
    ) -> Option<String> {
        debug_assert!(!self.kernel, "the browser compiler uses Effect");
        None
    }

    pub(super) fn crossed(&mut self, _toward: Toward, _shape: &Shape, value: String) -> String {
        debug_assert!(!self.kernel, "the browser compiler uses Effect");
        value
    }
}
