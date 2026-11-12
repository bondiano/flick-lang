//! The TypeScript forms of types: the backend's own of the built-in types,
//! by lang item, and those modules give their types with `@ts`, read from
//! their interfaces.

use std::cell::RefCell;
use std::sync::Arc;

use super::{Projection, lang_forms};
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

thread_local! {
    /// The forms of the built-in modules asked about so far: they are the
    /// same in every build, so they are read once, from the interfaces a
    /// caller has. Asking for the built-in modules here instead would check
    /// them all again when the caller had them checked another way, as an
    /// editor's projection has beside a build for Flick's own runtime.
    static BUILTIN: RefCell<Vec<(String, Forms)>> = const { RefCell::new(Vec::new()) };
}

impl Projection<'_> {
    /// The form `module` gives its type or service `name`, if it gives one
    /// or says there is none: such a type is never declared.
    pub(super) fn form(&self, module: &str, name: &str) -> Result<Option<Arc<Form>>, String> {
        if let Some(form) = lang_forms::of(module, name, self.rt) {
            return Ok(Some(form));
        }
        let find = |forms: &Forms| match forms {
            Ok(forms) => Ok((forms.iter().find(|(n, _)| n == name)).map(|(_, f)| Arc::clone(f))),
            Err(why) => Err(why.clone()),
        };
        let of = |known: &[(String, Forms)]| {
            (known.iter().find(|(m, _)| m == module)).map(|(_, forms)| find(forms))
        };
        if let Some(found) = BUILTIN.with_borrow(|known| of(known)) {
            return found;
        }
        if let Some(found) = of(&self.forms.borrow()) {
            return found;
        }
        let interface = (self.lookup)(module);
        let builtin =
            interface.is_some() && types::builtin_sources().iter().any(|(m, _)| *m == module);
        let forms = interface.map_or(Ok(Vec::new()), |i| read(&i));
        let found = find(&forms);
        let known = (module.to_owned(), forms);
        if builtin {
            BUILTIN.with_borrow_mut(|all| all.push(known));
        } else {
            self.forms.borrow_mut().push(known);
        }
        found
    }
}
