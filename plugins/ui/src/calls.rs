//! What the types alone show about the calls of `Ui` the package watches:
//! the capacity of an event policy, which must be a positive integer.

use flick_plugin::{CallSite, Constant, Diagnostic};

/// `U001` on each `queue c` or `concurrent c` of `Ui.Event` or `Action`
/// whose constant capacity or limit is no positive integer; a computed one
/// is checked when the handler is made.
pub fn check(calls: &[CallSite]) -> Vec<Diagnostic> {
    let mut found: Vec<_> = calls.iter().filter_map(one).collect();
    found.extend(atom_keys(calls));
    found
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
            }],
            refinement: None,
            binding: None,
        }
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
