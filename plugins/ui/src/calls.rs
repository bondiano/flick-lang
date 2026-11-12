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
    found.extend(calls.iter().filter_map(made_in_render));
    found.extend(calls.iter().filter_map(beyond_its_row));
    found.extend(calls.iter().filter_map(atom_of_every_type));
    found
}

/// `U005` on a value that holds an atom at every type its variables stand
/// for: the compiler refuses a cell of a type for every type, and this
/// says it of an atom.
fn atom_of_every_type(call: &CallSite) -> Option<Diagnostic> {
    let tagged = call.tagged.as_ref().filter(|t| t.tag == "cell")?;
    if !matches!(call.module.as_str(), "Atom" | "Reactivity.Atom") {
        return None;
    }
    let name = call.binding.as_ref().and_then(|b| b.name.as_deref());
    let (atom, it) = name.map_or_else(
        || ("this atom".to_owned(), "it".to_owned()),
        |n| (format!("the atom `{n}`"), format!("`{n}`")),
    );
    let message = format!(
        "{atom} has the type `{}` for every type its variables stand for, yet holds one value: give {it} the type of its value, without variables",
        tagged.named
    );
    Some(Diagnostic::error(message, call.span).with_code("U005"))
}

/// `U004` on what a render asks for besides reading atoms, and `E002` in
/// the words of a component on what its setup asks for besides `Setup`:
/// the compiler refuses both as a row that does not allow a service, and
/// these say where the code belongs instead.
fn beyond_its_row(call: &CallSite) -> Option<Diagnostic> {
    let lacking = call.lacking.as_ref()?;
    let (what, allowed) = (&lacking.what, &lacking.allowed);
    if allowed.open {
        return None;
    }
    if allowed.services == ["Prelude.Setup"] {
        let message =
            format!("a component's setup runs once per instance, so {what} is not allowed in it");
        let fix = "read atoms in the component's last expression, its render, as `ui { let x = … }`, or make an `Atom.derived` here";
        let refusal = Diagnostic::error(message, call.span).with_code("E002");
        return Some(refusal.with_fix(fix, Vec::new(), false));
    }
    if allowed.services != ["Atom.Read"] {
        return None;
    }
    let is = |label: &str| lacking.labels.iter().any(|l| l == label);
    let writes = is("Atom.Write") || is("Reactivity.Atom.Registry");
    let (setup, document) = (is("Prelude.Setup"), is("Ui.Document"));
    let fixes = [
        setup.then_some("call it before a component's last expression, in its setup, which runs once per instance"),
        document.then_some("reach the DOM in a task: of an `Action`, `Ui.on_mount` or `Ui.watch`"),
        writes.then_some("write atoms in an `Action`, which runs on an event"),
        (!writes && !setup && !document && !lacking.services.is_empty())
            .then_some("run the task as `Atom.task` and read its result"),
        (!lacking.errors.is_empty()).then_some(
            "handle the failure here, or render with `Ui.suspended`, which gives it a view",
        ),
    ];
    let fix = fixes.into_iter().flatten().collect::<Vec<_>>().join("; ");
    let message =
        format!("a view's render and a derived atom only read atoms: {what} is not allowed here");
    let refusal = Diagnostic::error(message, call.span).with_code("U004");
    Some(refusal.with_fix(&fix, Vec::new(), false))
}

/// `U006` on an atom, an action or a view made while rendering: a view
/// renders again at any time, and each render would make another one,
/// losing its state, its subscribers or its policy. A render is a function
/// whose services are `Atom.Read` alone; the task of a handler in it runs
/// on an event, and the compiler leaves it out of what is around the place.
fn made_in_render(call: &CallSite) -> Option<Diagnostic> {
    let tagged = call.tagged.as_ref().filter(|t| t.tag == "maker")?;
    let render = (tagged.inside.iter()).any(|f| !f.open && f.services == ["Atom.Read"]);
    render.then(|| {
        let what = if tagged.named == tagged.reaches {
            format!("`{}`", tagged.named)
        } else {
            format!("`{}`, which calls `{}`,", tagged.named, tagged.reaches)
        };
        let message = format!("{what} while rendering makes another one on every render: make it once, with a top-level `let`, or one per key with `Atom.family`");
        Diagnostic::error(message, call.span).with_code("U006")
    })
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
        // every watched name comes with its binding: these are `Atom`'s
        let binding = call.binding.as_ref().filter(|_| call.module == "Atom")?;
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
mod tests;
