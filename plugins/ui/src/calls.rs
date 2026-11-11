//! What the types alone show about the calls of `Ui` the package watches:
//! the capacity of an event policy, which must be a positive integer, and a
//! handler made of a `fun` where a view renders.

use flick_plugin::{CallSite, Constant, Diagnostic, Span};

/// `U001` on each `queue c` or `concurrent c` of `Ui.Event` or `Action`
/// whose constant capacity or limit is no positive integer; a computed one
/// is checked when the handler is made.
pub fn check(calls: &[CallSite]) -> Vec<Diagnostic> {
    let mut found: Vec<_> = calls.iter().filter_map(one).collect();
    found.extend(atom_keys(calls));
    found.extend(handlers_in_render(calls));
    found.extend(calls.iter().filter_map(reads_atoms_for_a_ui));
    found
}

/// `U007` on a top-level function whose signature reads atoms and which
/// gives a `Ui`: whatever calls it renders again with it, where a
/// `component` would render on its own.
fn reads_atoms_for_a_ui(call: &CallSite) -> Option<Diagnostic> {
    let binding = call.binding.as_ref()?;
    let gives = binding.gives.as_ref()?;
    let reads = call.module == "Atom" && call.name == "Read";
    (reads && gives.exact.as_deref() == Some("Prelude.Ui")).then(|| {
        let name = binding.name.as_deref().unwrap_or_default();
        let message = format!(
            "`{name}` reads atoms to build a `Ui`: whatever calls it renders again when they change"
        );
        let fix = "make it a `component`: it renders again on its own, not with its parent";
        Diagnostic::warning(message, call.span)
            .with_code("U007")
            .with_fix(fix, Vec::new(), false)
    })
}

/// The constructors of `Ui.Event`, which make a handler of a function.
const EVENTS: [&str; 4] = ["exhaust", "latest", "queue", "concurrent"];

/// `U002` on each handler made of a `fun` inside the render of a `Ui.view`
/// the module writes: a new closure on every render. A `component` is a
/// view its plugin writes, and its handlers stay as they are.
fn handlers_in_render(calls: &[CallSite]) -> Vec<Diagnostic> {
    let renders: Vec<Span> = (calls.iter())
        .filter(|call| call.module == "Ui" && call.name == "view" && !call.generated)
        .filter_map(|view| view.arguments.first().map(|render| render.span))
        .collect();
    let within = |at: Span| (renders.iter()).any(|r| r.start <= at.start && at.end <= r.end);
    (calls.iter())
        .filter(|call| call.module == "Ui.Event" && EVENTS.contains(&call.name.as_str()))
        .filter_map(|call| {
            let last = call.arguments.last().filter(|last| last.lambda)?;
            let span = Span {
                start: call.span.start,
                end: last.span.end,
            };
            within(span).then(|| {
                let message = format!("`{}.{}` of a `fun` makes a handler on every render: declare an `Action` at the top level, or pass data with `Action.with`", call.module, call.name);
                Diagnostic::warning(message, span).with_code("U002")
            })
        })
        .collect()
}

fn one(call: &CallSite) -> Option<Diagnostic> {
    let what = match call.name.as_str() {
        "queue" => "capacity",
        "concurrent" => "limit",
        _ => return None,
    };
    if !matches!(call.module.as_str(), "Ui.Event" | "Action") {
        return None;
    }
    let argument = call.arguments.first()?;
    let Some(Constant::Number(n)) = argument.constant else {
        return None;
    };
    (!(n >= 1.0 && n.fract() == 0.0 && n.is_finite())).then(|| {
        let message = format!(
            "the {what} of `{}.{}` must be a positive integer",
            call.module, call.name
        );
        Diagnostic::error(message, argument.span).with_code("U001")
    })
}

/// Serializable atoms use one key per unconstrained top-level binding.
fn atom_keys(calls: &[CallSite]) -> Vec<Diagnostic> {
    let mut defined = Vec::new();
    calls.iter().filter_map(|call| {
        let binding = call.binding.as_ref()?;
        let arity = match call.name.as_str() {
            "serializable" | "serializable_family" | "serializable_task" => 2,
            "serializable_result" => 3,
            _ => return None,
        };
        let name = &call.name;
        let message = match &binding.name {
            None => format!("`Atom.{name}` takes its key from the top-level binding it defines: define the atom with a top-level `let` without parameters"),
            Some(b) if binding.constrained => format!("`Atom.{name}` in `{b}`, whose type takes a dictionary, would make an atom at each use of `{b}`: drop the constraint from its signature"),
            Some(_) if binding.in_function => format!("`Atom.{name}` inside a function would give its key to every atom the function makes: define the atom with a top-level `let`"),
            Some(_) if binding.applied != arity => format!("`Atom.{name}` is applied to its codec and state where it is defined, since the binding's name is its key"),
            Some(b) if defined.contains(&(&binding.module, &binding.name, binding.span)) => format!("a second serializable atom in `{b}` would share its key `{}.{b}`: define each with its own top-level `let`", binding.module),
            Some(_) => { defined.push((&binding.module, &binding.name, binding.span)); return None; }
        };
        Some(Diagnostic::error(message, call.span).with_code("U003"))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::check;
    use flick_plugin::{Argument, CallSite, Constant, FactType, Span};

    fn call(module: &str, name: &str, constant: Option<Constant>) -> CallSite {
        CallSite {
            module: module.to_owned(),
            name: name.to_owned(),
            span: Span { start: 0, end: 5 },
            arguments: vec![Argument {
                span: Span { start: 6, end: 7 },
                ty: FactType {
                    shown: "Int".to_owned(),
                    exact: Some("Prelude.Int".to_owned()),
                    literal_of: None,
                },
                fields: None,
                constant,
                lambda: false,
            }],
            refinement: None,
            binding: None,
            generated: false,
        }
    }

    /// `Ui.view (fun _ -> … Ui.Event.exhaust (fun _ -> …) …)`, the view at 0.
    fn view_with_handler(generated: bool, lambda: bool) -> [CallSite; 2] {
        let mut view = call("Ui", "view", None);
        view.arguments[0].span = Span { start: 8, end: 60 };
        view.generated = generated;
        let mut handler = call("Ui.Event", "exhaust", None);
        handler.span = Span { start: 20, end: 36 };
        handler.arguments[0].span = Span { start: 37, end: 55 };
        handler.arguments[0].lambda = lambda;
        [view, handler]
    }

    #[test]
    fn a_function_that_reads_atoms_for_a_ui_is_told_of_components() {
        use flick_plugin::Binding;
        let named = |gives: &str| {
            let mut site = call("Atom", "Read", None);
            site.arguments.clear();
            site.binding = Some(Binding {
                module: "App".into(),
                name: Some("row".into()),
                span: Span { start: 0, end: 30 },
                constrained: false,
                in_function: false,
                applied: 0,
                gives: Some(FactType {
                    shown: gives.rsplit('.').next().unwrap_or(gives).to_owned(),
                    exact: Some(gives.to_owned()),
                    literal_of: None,
                }),
            });
            site
        };
        let found = check(&[named("Prelude.Ui")]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code.as_deref(), Some("U007"));
        assert!(found[0].message.starts_with("`row` reads atoms"));
        // a derived value is no view
        assert!(check(&[named("Prelude.Int")]).is_empty());
    }

    #[test]
    fn a_handler_of_a_fun_in_a_written_render_is_made_on_every_render() {
        let found = check(&view_with_handler(false, true));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code.as_deref(), Some("U002"));
        assert_eq!(found[0].span, Span { start: 20, end: 55 });
        // a named function is made once, and a `component` renders as it did
        assert!(check(&view_with_handler(false, false)).is_empty());
        assert!(check(&view_with_handler(true, true)).is_empty());
        // outside a render nothing renders again
        let [_, handler] = view_with_handler(false, true);
        assert!(check(&[handler]).is_empty());
    }

    #[test]
    fn serializable_keys_are_checked_from_binding_facts() {
        use flick_plugin::Binding;
        let mut site = call("Atom", "serializable", None);
        site.binding = Some(Binding {
            module: "App".into(),
            name: Some("count".into()),
            span: Span { start: 0, end: 30 },
            constrained: false,
            in_function: false,
            applied: 2,
            gives: None,
        });
        assert!(check(&[site.clone()]).is_empty());
        let mut second = site.clone();
        second.span = Span { start: 31, end: 48 };
        let found = check(&[site.clone(), second]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code.as_deref(), Some("U003"));
        assert_eq!(found[0].span, Span { start: 31, end: 48 });
        assert!(found[0].message.contains("App.count"));
        for (name, constrained, in_function, applied, message) in [
            (None, false, false, 2, "top-level binding"),
            (Some("count"), true, false, 2, "takes a dictionary"),
            (Some("count"), false, true, 2, "inside a function"),
            (Some("count"), false, false, 1, "codec and state"),
        ] {
            let mut invalid = site.clone();
            let binding = invalid.binding.as_mut().expect("binding");
            binding.name = name.map(str::to_owned);
            binding.constrained = constrained;
            binding.in_function = in_function;
            binding.applied = applied;
            let found = check(&[invalid]);
            assert_eq!(found.len(), 1);
            assert!(found[0].message.contains(message), "{:?}", found[0]);
        }
    }

    #[test]
    fn a_policy_takes_a_positive_integer() {
        let refused = |n: f64| {
            let found = check(&[call("Ui.Event", "queue", Some(Constant::Number(n)))]);
            found.first().map(|d| d.message.clone())
        };
        assert_eq!(
            refused(0.0).as_deref(),
            Some("the capacity of `Ui.Event.queue` must be a positive integer")
        );
        assert!(refused(1.5).is_some());
        assert!(refused(-1.0).is_some());
        assert!(refused(1.0).is_none());
        // a computed bound is checked when the handler is made
        assert!(check(&[call("Ui.Event", "queue", None)]).is_empty());
        // another `queue`, of no policy, is none of this plugin's business
        assert!(check(&[call("App.Jobs", "queue", Some(Constant::Number(0.0)))]).is_empty());
        let limit = check(&[call("Action", "concurrent", Some(Constant::Number(0.0)))]);
        assert_eq!(
            limit.first().map(|d| d.message.clone()).as_deref(),
            Some("the limit of `Action.concurrent` must be a positive integer")
        );
    }
}
