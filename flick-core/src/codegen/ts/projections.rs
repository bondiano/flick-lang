//! The TypeScript forms modules give their types with `@ts`, read from
//! their interfaces: the backend names no type of a library.

use std::sync::{Arc, OnceLock};

use super::Projection;
use crate::iface::Interface;
use crate::syntax::ts_mark::{self, Form};
use crate::types;

/// The forms of a module by type, or why its interface gives one that does
/// not read.
pub(super) type Forms = Result<Vec<(String, Arc<Form>)>, String>;

fn read(interface: &Interface) -> Forms {
    let mut forms = Vec::new();
    for (name, parts) in interface.tags.get(ts_mark::TAG).into_iter().flatten() {
        match ts_mark::form(parts) {
            Ok(form) => forms.push((name.clone(), Arc::new(form))),
            Err(why) => {
                return Err(format!(
                    "the interface of `{}` gives `{name}` no TypeScript form that reads: {why}",
                    interface.module
                ));
            }
        }
    }
    Ok(forms)
}

/// The forms of the built-in modules, read once.
fn builtin() -> &'static [(String, Forms)] {
    static TABLE: OnceLock<Vec<(String, Forms)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (types::builtins().iter())
            .map(|interface| (interface.module.clone(), read(interface)))
            .collect()
    })
}

impl Projection<'_> {
    /// The form `module` gives its type or service `name`, if it gives one
    /// or says there is none: such a type is never declared.
    pub(super) fn form(&self, module: &str, name: &str) -> Result<Option<Arc<Form>>, String> {
        let find = |forms: &Forms| match forms {
            Ok(forms) => Ok((forms.iter().find(|(n, _)| n == name)).map(|(_, f)| Arc::clone(f))),
            Err(why) => Err(why.clone()),
        };
        if let Some((_, forms)) = builtin().iter().find(|(m, _)| m == module) {
            return find(forms);
        }
        if let Some((_, forms)) = self.forms.borrow().iter().find(|(m, _)| m == module) {
            return find(forms);
        }
        let forms = (self.lookup)(module).map_or(Ok(Vec::new()), |i| read(&i));
        let found = find(&forms);
        self.forms.borrow_mut().push((module.to_owned(), forms));
        found
    }
}
