//! The runner of an app: a module that exports `Main`, the type `main` of
//! each entry has, and `run`, which starts it on its host.

use super::Loader;
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::iface::Namespace;
use crate::resolve::{self, Interface};
use crate::source::Span;
use crate::types::{Form, show_type};
use crate::{syntax, types};

impl Loader<'_> {
    /// The interface of the runner `name`: a module of the program, of an
    /// installed package or a built-in one.
    fn runner_interface(
        &mut self,
        name: &str,
        interfaces: &[Option<Interface>],
    ) -> Option<Interface> {
        if let Some(&i) = self.by_name.get(name) {
            return interfaces.get(i).cloned().flatten();
        }
        if let Some(interface) = self.from_packages.get(name) {
            return Some(interface.clone());
        }
        let (owner, interface) = match self
            .packages
            .find(self.host, name, &mut self.diagnostics)
            .pop()
        {
            Some((package, interface)) => (Some(package), interface),
            None => (
                None,
                types::builtins().iter().find(|i| i.module == name)?.clone(),
            ),
        };
        if let Some(package) = owner {
            self.owners.insert(name.to_owned(), package);
        }
        self.from_packages
            .insert(name.to_owned(), interface.clone());
        Some(interface)
    }

    /// P026 unless `main` of `module`, at `main_at`, has the type `Main` of
    /// the runner `runner`, named at `runner_at`: the checker decides, on a
    /// module that binds the one to the other.
    pub(super) fn check_runner(
        &mut self,
        module: &str,
        main_at: Span,
        (runner, runner_at): (&str, Span),
        interfaces: &[Option<Interface>],
    ) {
        let error = |message: String, at: Span| Diagnostic::error(codes::P026, message, at);
        let Some(interface) = self.runner_interface(runner, interfaces) else {
            let message = format!(
                "runner `{runner}` not found: a built-in one such as `Runner.Node`, or a module of the program or of an installed package"
            );
            self.diagnostics.push(error(message, runner_at));
            return;
        };
        let Some(expected) = interface
            .find(Namespace::Type, "Main")
            .filter(|_| interface.exports_value("run"))
        else {
            let message = format!("runner `{runner}` does not export `Main` and `run`");
            self.diagnostics.push(error(message, runner_at));
            return;
        };
        let shown = match expected.definition.as_ref().map(|d| &d.form) {
            Some(Form::Alias { body, .. }) => format!(", `{}`", show_type(body)),
            _ => String::new(),
        };
        let text = format!(
            "module Flick.Runner exposing (check)\n\nimport {runner}\nimport {module}\n\ncheck : {runner}.Main\nlet check = {module}.main\n"
        );
        let file = self.sources.add(format!("<runner {runner}>"), text);
        let text = &self.sources.get(file).text;
        let parse = syntax::parse(text, file);
        let syntax = resolve::module_syntax(&parse.tree, text, file);
        let checked = resolve::Module {
            name: "Flick.Runner",
            file,
            text,
            tree: &parse.tree,
            syntax: &syntax,
        };
        let lookup = |name: &str| match self.by_name.get(name) {
            Some(&i) => interfaces.get(i).cloned().flatten(),
            None => self.from_packages.get(name).cloned(),
        };
        let (mut interface, mut diagnostics) = resolve::resolve(&checked, &lookup);
        let options = types::Options {
            local: &|_| false,
            limits: types::Limits::default(),
            sql: Default::default(),
            effects: true,
        };
        diagnostics.extend(types::check(
            &checked,
            &lookup,
            types::builtins(),
            options,
            &mut interface,
        ));
        if let Some(problem) = diagnostics.iter().find(|d| d.severity == Severity::Error) {
            let message = format!(
                "entry `main` is not a `{runner}.Main`{shown}: {}",
                problem.message
            );
            self.diagnostics.push(error(message, main_at));
        }
    }
}
