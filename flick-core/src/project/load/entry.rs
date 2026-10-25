//! The entry module of an app.

use std::path::Path;

use super::{Loader, Root};
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::project::config::Kind;
use crate::project::{is_module_name, module_path};
use crate::resolve::Interface;
use crate::source::Span;
use crate::types::{Effects, Type, show_type};

/// How the runner starts `main`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainKind {
    /// `Unit -> Unit`: called, with no effect runtime.
    Pure,
    /// `Unit ->{; AppError} Unit`: run as an effect.
    Effect,
}

/// How a runner starts `main : t`, or why none can: it takes and returns
/// `Unit`, needs no service and fails only with `AppError`.
pub fn main_kind(t: &Type) -> Result<MainKind, String> {
    let shape = || {
        format!(
            "entry `main` has type `{}`; a runner starts `Unit -> Unit` or `Unit ->{{; AppError}} Unit`",
            show_type(t)
        )
    };
    let body = match t {
        Type::Forall { body, .. } => body,
        _ => t,
    };
    let Type::Fun {
        param,
        effects,
        result,
    } = body
    else {
        return Err(shape());
    };
    if !param.is_prim("Unit") || !result.is_prim("Unit") {
        return Err(shape());
    }
    let Some(effects) = effects else {
        return Ok(MainKind::Pure);
    };
    let Effects { services, errors } = effects.as_ref();
    match services {
        Type::Var(_) => {}
        Type::Row(row) if row.entries.is_empty() => {}
        _ => {
            return Err(format!(
                "entry `main` needs `{}`, which no runner provides; provide it inside `main`",
                show_type(services)
            ));
        }
    }
    match errors {
        Type::Var(_) => {}
        Type::Row(row) if row.entries.iter().all(|(_, e)| e.is_prim("AppError")) => {}
        _ => {
            return Err(format!(
                "entry `main` can fail with `{}`; the runner reports only `AppError`, so handle the others inside `main`",
                show_type(errors)
            ));
        }
    }
    Ok(MainKind::Effect)
}

impl Loader<'_> {
    /// The entry of an app is a source module that exports `main`.
    pub(super) fn check_entry(&mut self, interfaces: &[Option<Interface>]) {
        let Some(project) = self.project.filter(|p| p.config.kind == Kind::App) else {
            return;
        };
        let Some((entry, span)) = project.config.entry.clone() else {
            return;
        };
        let error = |message: String| Diagnostic::error(codes::P011, message, span);
        if !is_module_name(&entry) {
            self.diagnostics
                .push(error(format!("`{entry}` is not a module name")));
            return;
        }
        let expected = project.source_root().join(module_path(&entry));
        let shown = Path::new(&project.config.source_dir).join(module_path(&entry));
        match self.by_name.get(&entry).copied() {
            Some(i) if self.modules[i].root == Root::Source => {
                let interface = interfaces[i].as_ref();
                if !interface.is_some_and(|i| i.exports_value("main")) {
                    self.diagnostics.push(
                        error(format!("entry module `{entry}` does not export `main`")).with_fix(
                            format!("add `main` to `module {entry} exposing (main)`"),
                            Vec::new(),
                        ),
                    );
                    return;
                }
                // no type when the module failed to check: that is reported already
                let main = interface.and_then(|i| i.find(Namespace::Value, "main"));
                if let Some((main, Err(message))) =
                    main.and_then(|m| Some((m, main_kind(m.scheme.as_ref()?))))
                {
                    let at = main.at.map_or(span, |[start, end]| {
                        Span::new(self.modules[i].file, start, end)
                    });
                    self.diagnostics
                        .push(Diagnostic::error(codes::P026, message, at));
                }
            }
            Some(_) => self.diagnostics.push(error(format!(
                "entry module `{entry}` is not in the source root: expected `{}`",
                shown.display()
            ))),
            // exists, but not among the modules this check covers
            None if self.host.read(&expected).is_ok() => {}
            None => self.diagnostics.push(error(format!(
                "entry module `{entry}` not found: expected `{}`",
                shown.display()
            ))),
        }
    }
}
