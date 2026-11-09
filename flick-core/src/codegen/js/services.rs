//! Services: the tag a service is named by, and its operations.

use super::{Gen, names};
use crate::ir::{Stage, operation_arity};

impl Gen<'_> {
    /// `S.op`, whose implementation is given `dicts` when it is looked up.
    pub(super) fn operation(
        &mut self,
        service: &str,
        name: &str,
        stages: &[Stage],
        dicts: &[String],
    ) -> String {
        let tag = self.service_ref(service);
        let op = match stages.iter().find(|&&s| s != Stage::Pure) {
            Some(Stage::Sync) if !self.kernel => self.rt("syncOperation"),
            _ => self.rt("operation"),
        };
        let arity = operation_arity(stages);
        let name = names::string(name);
        if dicts.is_empty() {
            format!("{op}({tag}, {name}, {arity})")
        } else {
            format!("{op}({tag}, {name}, {arity}, [{}])", dicts.join(", "))
        }
    }

    /// The tag of the service `key`, `M.S`.
    pub(super) fn service_ref(&mut self, key: &str) -> String {
        let (module, name) = key.rsplit_once('.').unwrap_or(("", key));
        self.global(module, &names::service(name))
    }
}
