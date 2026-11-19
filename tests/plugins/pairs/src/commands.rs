//! The commands of the fixture plugin: one of its own, and `greet`, which
//! the fixture backend also adds.

use flick_plugin::command::{
    self, Descriptor, Diagnostic, File, Invocation, Parameter, Plan, Style, ValueKind,
};

pub(super) fn commands() -> Vec<Descriptor> {
    ["pairs-hello", "greet"]
        .into_iter()
        .map(|name| Descriptor {
            name: name.into(),
            summary: format!("the pairs plugin's `{name}`"),
            positionals: vec![Parameter {
                name: "who".into(),
                kind: ValueKind::Text,
                multiple: false,
                required: true,
                help: "whom to greet".into(),
            }],
            options: vec![],
            inputs: vec![],
            reads: vec![],
            writes: vec!["pairs.txt".into()],
            style: Style::Plan,
        })
        .collect()
}

pub(super) fn run(invocation: &Invocation) -> Result<Plan, Vec<Diagnostic>> {
    if !commands().iter().any(|d| d.name == invocation.name) {
        return Err(command::unknown(&invocation.name));
    }
    let who = (invocation.arguments.iter())
        .find(|a| a.name == "who")
        .and_then(|a| a.values.first())
        .map_or("nobody", String::as_str);
    let mut plan = Plan::empty();
    plan.files.push(File {
        path: "pairs.txt".into(),
        bytes: format!("pairs {} {who}\n", invocation.name).into_bytes(),
        executable: false,
    });
    plan.stdout = format!("pairs greets {who}\n").into_bytes();
    Ok(plan)
}
