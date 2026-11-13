//! Services: the tag a service is named by, and its operations.

use super::doc::{Js, array, call};
use super::{Gen, names};
use crate::ir::{Stage, operation_arity};

impl<'d> Gen<'_, 'd> {
    /// `S.op`, whose implementation is given `dicts` when it is looked up.
    pub(super) fn operation(
        &mut self,
        service: &str,
        name: &str,
        stages: &[Stage],
        dicts: &[Js<'d>],
    ) -> Js<'d> {
        let tag = self.service_ref(service);
        let op = match stages.iter().find(|&&s| s != Stage::Pure) {
            Some(Stage::Sync) if !self.kernel => self.rt("syncOperation"),
            _ => self.rt("operation"),
        };
        let arity = operation_arity(stages);
        let name = self.string(name);
        let arity = self.leaf(&arity.to_string());
        if dicts.is_empty() {
            call(self.a, op, &[tag, name, arity])
        } else {
            call(self.a, op, &[tag, name, arity, array(self.a, dicts)])
        }
    }

    /// The tag of the service `key`, `M.S`.
    pub(super) fn service_ref(&mut self, key: &str) -> Js<'d> {
        let (module, name) = key.rsplit_once('.').unwrap_or(("", key));
        self.global(module, &names::service(name))
    }
}
