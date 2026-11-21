//! `NoControl r` and `Detached r`:
//!
//! - `NoControl r`: no effect of the services `r` has a `control` operation.
//!   A `lazy` initializer needs it so forcing it cannot split the forcing
//!   fiber's continuation, while `abort` remains allowed because the
//!   initializer runs synchronously in the forcing fiber.
//! - `Detached r`: no effect of the services `r` has a `control` or `abort`
//!   operation (with `NoControl r` as superclass). Work that runs apart from
//!   the code that started it needs it: a child of `spawn` or `parallel`, or
//!   what a region keeps, as a finalizer. Both `control` and `abort` unwind to
//!   the handler in the fiber that installed it, and a finalizer may run after
//!   that handler let its body go.

use super::Dict;
use super::args_of;
use super::solve::{Givens, Outcome};
use crate::types::Checker;
use crate::types::env::Form;
use crate::types::lang::Lang;
use crate::types::services::OpClass;
use crate::types::ty::Type;

impl Checker<'_> {
    pub(super) fn detached(
        &mut self,
        c: &Type,
        args: &[Type],
        givens: &Givens,
        class: Lang,
    ) -> Outcome {
        let [row] = args else {
            return Outcome::Missing(c.clone(), None);
        };
        let norm = self.store.normalize(row);
        for (_, entry) in &norm.entries {
            if class == Lang::NoControl {
                if let Some((effect, op)) = self.control_operation(entry) {
                    let reason = format!(
                        "effect `{effect}` has the `control` operation `{op}`, which cannot split a `lazy` initializer: handle `{effect}` inside it"
                    );
                    return Outcome::Missing(c.clone(), Some(reason));
                }
            } else if let Some((effect, op, kind)) = self.non_tail_operation(entry) {
                let kind = match kind {
                    OpClass::Control => "control",
                    OpClass::Abort => "abort",
                    OpClass::Tail => {
                        unreachable!("non_tail_operation only returns Control or Abort")
                    }
                };
                let reason = format!(
                    "effect `{effect}` has the `{kind}` operation `{op}`, and this task runs apart from its handler: handle `{effect}` inside the task"
                );
                return Outcome::Missing(c.clone(), Some(reason));
            }
        }
        for tail in &norm.tails {
            match tail {
                Type::Meta(_) => return Outcome::Stuck,
                Type::Error => {}
                rigid => {
                    let given = givens
                        .iter()
                        .any(|(g, _)| args_of(g, class).is_some_and(|a| a[..] == [rigid]));
                    if !given {
                        let part = Type::app(Type::lang(class), rigid.clone());
                        return Outcome::Missing(part, None);
                    }
                }
            }
        }
        Outcome::Solved(Dict::Primitive {
            constraint: self.show_exact(c),
            labels: Vec::new(),
        })
    }

    /// The effect of a row entry and one of its `control` operations.
    pub(in crate::types) fn control_operation(&self, entry: &Type) -> Option<(String, String)> {
        let Type::Con { module, name } = entry.spine().0 else {
            return None;
        };
        let Form::Service { operations, .. } = &self.env.type_def(module, name)?.form else {
            return None;
        };
        let op = operations.iter().find(|o| o.class == OpClass::Control)?;
        Some((name.clone(), op.name.clone()))
    }

    /// The effect of a row entry and one of its non-`tail` (`control` or
    /// `abort`) operations.
    fn non_tail_operation(&self, entry: &Type) -> Option<(String, String, OpClass)> {
        let Type::Con { module, name } = entry.spine().0 else {
            return None;
        };
        let Form::Service { operations, .. } = &self.env.type_def(module, name)?.form else {
            return None;
        };
        let op = operations
            .iter()
            .find(|o| matches!(o.class, OpClass::Control | OpClass::Abort))?;
        Some((name.clone(), op.name.clone(), op.class))
    }
}
