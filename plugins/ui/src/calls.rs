//! What the types alone show about the calls of `Ui` the package watches:
//! the capacity of an event policy, which must be a positive integer.

use flick_plugin::{CallSite, Constant, Diagnostic};

/// `U001` on each `queue c` or `concurrent c` of `Ui.Event` or `Action`
/// whose constant capacity or limit is no positive integer; a computed one
/// is checked when the handler is made.
pub fn check(calls: &[CallSite]) -> Vec<Diagnostic> {
    calls.iter().filter_map(one).collect()
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
