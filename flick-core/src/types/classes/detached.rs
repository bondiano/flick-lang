//! `Detached r`: no effect of the services `r` has a `control` operation.
//! Work that runs apart from the code that started it needs it: a child of
//! `spawn` or `parallel`, or what a region keeps, as a finalizer. The handler
//! of a `control` effect, and the rest of the computation that `resume` holds,
//! are in the fiber that installed the handler, and a finalizer may run after
//! that handler let its body go.

use super::Dict;
use super::args_of;
use super::solve::{Givens, Outcome};
use crate::types::Checker;
use crate::types::env::Form;
use crate::types::lang::Lang;
use crate::types::services::OpClass;
use crate::types::ty::Type;

impl Checker<'_> {
    pub(super) fn detached(&mut self, c: &Type, args: &[Type], givens: &Givens) -> Outcome {
        let [row] = args else {
            return Outcome::Missing(c.clone(), None);
        };
        let norm = self.store.normalize(row);
        for (_, entry) in &norm.entries {
            if let Some((effect, op)) = self.control_operation(entry) {
                let reason = format!(
                    "effect `{effect}` has the `control` operation `{op}`, and this task runs apart from its handler: handle `{effect}` inside the task"
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
                        .any(|(g, _)| args_of(g, Lang::Detached).is_some_and(|a| a[..] == [rigid]));
                    if !given {
                        let part = Type::app(Type::lang(Lang::Detached), rigid.clone());
                        return Outcome::Missing(part, None);
                    }
                }
            }
        }
        Outcome::Solved(Dict::Primitive {
            constraint: self.show(c),
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
}
