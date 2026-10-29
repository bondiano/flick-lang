//! The entry module of an app.

use std::path::Path;

use super::{Loader, Root, target};
use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::project::config::{self, Backend, Emit, Kind};
use crate::project::{Project, is_module_name, module_path};
use crate::resolve::Interface;
use crate::source::Span;
use crate::types::{self, Effects, NO_EFFECTS, Type, show_type};

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
    let Effects {
        services, errors, ..
    } = effects.as_ref();
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
    /// Each entry of an app is a source module that exports `main`, its own
    /// or re-exported, of a type a runner can start.
    pub(super) fn check_entry(&mut self, interfaces: &[Option<Interface>]) {
        let Some(project) = self.project.filter(|p| p.config.kind == Kind::App) else {
            return;
        };
        for entry in &project.config.entries {
            self.check_one_entry(project, entry, interfaces);
        }
    }

    fn check_one_entry(
        &mut self,
        project: &Project,
        entry: &config::Entry,
        interfaces: &[Option<Interface>],
    ) {
        let (span, runner, client) = (entry.span, entry.runner.as_ref(), entry.client);
        let (name, entry) = (entry.name.as_str(), entry.module.as_str());
        let error = |message: String| Diagnostic::error(codes::P011, message, span);
        if !is_module_name(entry) {
            self.diagnostics
                .push(error(format!("`{entry}` is not a module name")));
            return;
        }
        let expected = project.source_root().join(module_path(entry));
        let shown = Path::new(&project.config.source_dir).join(module_path(entry));
        match self.by_name.get(entry).copied() {
            Some(i) if self.modules[i].root == Root::Source => {
                let interface = interfaces[i].as_ref();
                // `main` may be re-exported: the runner imports it from the module that defines it
                let Some(main) = interface.and_then(|i| i.find(Namespace::Value, "main")) else {
                    if interface.is_some() {
                        self.diagnostics.push(
                            error(format!("entry module `{entry}` does not export `main`"))
                                .with_fix(|| {
                                    (
                                        format!("add `main` to `module {entry} exposing (main)`"),
                                        Vec::new(),
                                    )
                                }),
                        );
                    }
                    return;
                };
                let defining = self.by_name.get(&main.entity.module).copied();
                let at = main.at.zip(defining).map_or(span, |([start, end], d)| {
                    Span::new(self.modules[d].file, start, end)
                });
                if client && let Some(path) = self.server_path(&main.entity.module, interfaces) {
                    let message = format!(
                        "client entry `{name}` reaches `Server`: {}",
                        path.iter()
                            // the implementation of a server function is how a program names it
                            .map(|v| match v.strip_suffix("$server") {
                                Some(f) => format!("`server {f}`"),
                                None => format!("`{v}`"),
                            })
                            .collect::<Vec<_>>()
                            .join(" → ")
                    );
                    self.diagnostics.push(
                        Diagnostic::error(codes::E009, message, at).with_fix(|| (
                            "call what needs `Server` from a server entry, and reach it from the client over HTTP or RPC",
                            Vec::new(),
                        )),
                    );
                }
                // no type when the module failed to check: that is reported already
                if let Some((runner, runner_at)) = runner
                    && project.config.build.emit == Emit::Go
                    && target::js_only(runner)
                {
                    self.diagnostics
                        .push(target::js_only_error(runner, *runner_at));
                    return;
                }
                if let Some((runner, runner_at)) = runner
                    && main.scheme.is_some()
                {
                    let module = main.entity.module.clone();
                    self.check_runner(&module, at, (runner, *runner_at), interfaces);
                    return;
                }
                let pure = project.config.build.effect == Backend::None;
                let problem = main.scheme.as_ref().and_then(|t| match main_kind(t) {
                    Err(message) => Some(message),
                    Ok(MainKind::Effect) if pure => Some(format!(
                        "entry `main` has type `{}`, but {NO_EFFECTS}: a runner calls `Unit -> Unit`",
                        show_type(t)
                    )),
                    Ok(_) => None,
                });
                if let Some(message) = problem {
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

    /// How `main` of `module` reaches `Server`, value by value, from the
    /// paths the interfaces record; `None` when it does not.
    fn server_path(&self, module: &str, interfaces: &[Option<Interface>]) -> Option<Vec<String>> {
        let interface_of = |m: &str| {
            let own = self.by_name.get(m).and_then(|&i| interfaces[i].as_ref());
            own.or_else(|| self.from_packages.get(m))
                .or_else(|| types::builtins().iter().find(|i| i.module == m))
        };
        let (mut module, mut name) = (module.to_owned(), "main".to_owned());
        let mut path = vec![format!("{module}.{name}")];
        // a value whose own type needs `Server` lists no path: it ends there
        while let Some((_, hops)) =
            interface_of(&module).and_then(|i| i.servers.iter().find(|(n, _)| *n == name))
        {
            for hop in hops {
                match hop.rsplit_once('.') {
                    Some((m, n)) => (module, name) = (m.to_owned(), n.to_owned()),
                    None => name.clone_from(hop),
                }
                path.push(format!("{module}.{name}"));
            }
            let last_is_local = hops.last().is_none_or(|h| !h.contains('.'));
            if last_is_local || path.len() > 64 {
                break;
            }
        }
        (path.len() > 1).then_some(path)
    }
}
