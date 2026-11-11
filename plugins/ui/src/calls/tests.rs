//! The checks of calls, each on the facts a module would give.

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
        tagged: None,
        lacking: None,
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
fn a_cell_of_every_type_is_told_of_an_atom_and_of_no_other_cell() {
    use flick_plugin::{Binding, Tagged};
    let held = |module: &str, name: Option<&str>| {
        let mut site = call(module, "Writable", None);
        site.arguments.clear();
        site.binding = Some(Binding {
            module: "App".into(),
            name: name.map(str::to_owned),
            span: site.span,
            constrained: false,
            in_function: false,
            applied: 0,
            gives: None,
        });
        site.tagged = Some(Tagged {
            tag: "cell".into(),
            named: "Writable (List a)".into(),
            reaches: format!("{module}.Writable"),
            inside: Vec::new(),
        });
        check(&[site])
    };
    let named = held("Atom", Some("items"));
    assert_eq!(named[0].code.as_deref(), Some("U005"));
    assert!(
        named[0]
            .message
            .starts_with("the atom `items` has the type `Writable (List a)`")
    );
    assert!(
        named[0]
            .message
            .ends_with("give `items` the type of its value, without variables")
    );
    let unnamed = held("Reactivity.Atom", None);
    assert!(unnamed[0].message.starts_with("this atom has the type"));
    // the cell of another library is the compiler's to refuse
    assert!(held("App.Slots", Some("slot")).is_empty());
}

#[test]
fn what_a_render_or_a_setup_asks_for_beyond_its_row_is_told_where_it_belongs() {
    use flick_plugin::{Enclosing, Lacking};
    let under = |allowed: &str, open: bool, labels: &[&str]| {
        let mut site = call("Atom", "Read", None);
        site.arguments.clear();
        site.lacking = Some(Lacking {
            allowed: Enclosing {
                services: vec![allowed.to_owned()],
                open,
            },
            what: "service `Write`".into(),
            services: vec!["Write".into()],
            labels: labels.iter().map(|l| (*l).to_owned()).collect(),
            errors: Vec::new(),
        });
        check(&[site])
    };
    let render = under("Atom.Read", false, &["Atom.Write"]);
    assert_eq!(render[0].code.as_deref(), Some("U004"));
    assert!(
        render[0]
            .message
            .ends_with("service `Write` is not allowed here")
    );
    assert_eq!(
        render[0].fixes[0].message,
        "write atoms in an `Action`, which runs on an event"
    );
    // a service the library does not know is a task to run as an atom
    let task = under("Atom.Read", false, &["App.Log"]);
    assert!(
        task[0].fixes[0]
            .message
            .starts_with("run the task as `Atom.task`")
    );
    let setup = under("Prelude.Setup", false, &["Atom.Read"]);
    assert_eq!(setup[0].code.as_deref(), Some("E002"));
    assert!(
        setup[0]
            .message
            .starts_with("a component's setup runs once")
    );
    // a row open to more, and a row of another service, are the compiler's
    assert!(under("Atom.Read", true, &["Atom.Write"]).is_empty());
    assert!(under("App.Log", false, &["Atom.Write"]).is_empty());
}

#[test]
fn a_maker_named_in_a_render_is_refused_and_one_in_a_task_is_not() {
    use flick_plugin::{Enclosing, Tagged};
    let named = |named: &str, inside: &[(&[&str], bool)]| {
        let mut site = call("Atom", "make", None);
        site.arguments.clear();
        site.tagged = Some(Tagged {
            tag: "maker".into(),
            named: named.into(),
            reaches: "Atom.make".into(),
            inside: (inside.iter())
                .map(|(services, open)| Enclosing {
                    services: services.iter().map(|s| (*s).to_owned()).collect(),
                    open: *open,
                })
                .collect(),
        });
        site
    };
    let render: (&[&str], bool) = (&["Atom.Read"], false);
    let found = check(&[named("Atom.make", &[(&[], false), render])]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code.as_deref(), Some("U006"));
    assert!(found[0].message.starts_with("`Atom.make` while rendering"));
    let through = check(&[named("fresh", &[render])]);
    assert!(
        through[0]
            .message
            .starts_with("`fresh`, which calls `Atom.make`,")
    );
    // a function that may do more is no render, nor is one that asks for more
    assert!(check(&[named("fresh", &[(&["Atom.Read"], true)])]).is_empty());
    assert!(check(&[named("fresh", &[(&["Atom.Read", "Atom.Write"], false)])]).is_empty());
    assert!(check(&[named("fresh", &[])]).is_empty());
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
