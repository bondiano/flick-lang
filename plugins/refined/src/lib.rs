//! The plugin of std for refined types. `T.parse c` of a constant `c` is
//! checked when the program compiles wherever the compiler knows the check
//! of `T`: the literals of an enum, one of the checks of `Checks`, or the
//! verdict of the compiler on the module's own predicate.

use flick_plugin::{CallSite, Constant, Diagnostic, Refinement};

/// `D006` on each `T.parse c` whose constant never passes the check of `T`.
pub fn check(calls: &[CallSite]) -> Vec<Diagnostic> {
    calls.iter().filter_map(one).collect()
}

fn one(call: &CallSite) -> Option<Diagnostic> {
    let ty = call.name.strip_suffix("$parse")?;
    let argument = call.arguments.first()?;
    let constant = argument.constant.as_ref()?;
    let fails =
        format!("this constant fails the check of `{ty}`, so `{ty}.parse` of it always fails");
    let message = match call.refinement.as_ref()? {
        Refinement::Literals(literals) if !literal_of(literals, constant)? => {
            format!("this constant is no literal of `{ty}`, so `{ty}.parse` of it always fails")
        }
        Refinement::Known(check) if !passes(check, constant)? => fails,
        Refinement::Holds(false) => fails,
        _ => return None,
    };
    Some(Diagnostic::error(message, argument.span).with_code("D006"))
}

/// Whether `constant` is one of the literals of an enum; none for a
/// constant of another kind, which the types report on their own.
fn literal_of(literals: &[Constant], constant: &Constant) -> Option<bool> {
    match constant {
        Constant::Text(s) => {
            Some((literals.iter()).any(|l| matches!(l, Constant::Text(literal) if literal == s)))
        }
        // exactly the integer, as the bits compare it
        Constant::Number(n) => {
            Some((literals.iter()).any(
                |l| matches!(l, Constant::Number(literal) if literal.to_bits() == n.to_bits()),
            ))
        }
        Constant::Boolean(_) => None,
    }
}

/// Whether `constant` passes the check the compiler knows as `name`, as the
/// runtime applies it; none for a constant of another kind.
fn passes(name: &str, constant: &Constant) -> Option<bool> {
    match (name, constant) {
        ("email", Constant::Text(s)) => Some(email(s)),
        ("non_empty", Constant::Text(s)) => Some(!s.is_empty()),
        ("positive", Constant::Number(n)) => Some(*n > 0.0),
        _ => None,
    }
}

/// `Checks.email`, as the runtime checks it: one `@` with text on both
/// sides, no spaces, and a dot inside the domain.
fn email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !s.chars().any(char::is_whitespace)
        && !domain.contains('@')
        && domain
            .char_indices()
            .any(|(i, c)| c == '.' && i > 0 && i + 1 < domain.len())
}

#[cfg(test)]
mod tests {
    use std::slice;

    use super::check;
    use flick_plugin::{Argument, CallSite, Constant, FactType, Refinement, Span};

    fn call(name: &str, constant: Constant, refinement: Option<Refinement>) -> CallSite {
        CallSite {
            module: "App.Users".to_owned(),
            name: name.to_owned(),
            span: Span { start: 0, end: 11 },
            arguments: vec![Argument {
                span: Span { start: 12, end: 20 },
                ty: FactType {
                    shown: "String".to_owned(),
                    exact: Some("Prelude.String".to_owned()),
                    literal_of: None,
                },
                fields: None,
                constant: Some(constant),
            }],
            refinement,
            binding: None,
        }
    }

    fn message(call: &CallSite) -> Option<String> {
        check(slice::from_ref(call))
            .first()
            .map(|d| d.message.clone())
    }

    #[test]
    fn a_constant_of_an_enum_is_one_of_its_literals() {
        let literals = Refinement::Literals(vec![
            Constant::Text("GET".to_owned()),
            Constant::Text("POST".to_owned()),
        ]);
        let bad = call(
            "Method$parse",
            Constant::Text("PATCH".to_owned()),
            Some(literals.clone()),
        );
        assert_eq!(
            message(&bad).as_deref(),
            Some("this constant is no literal of `Method`, so `Method.parse` of it always fails")
        );
        let good = call(
            "Method$parse",
            Constant::Text("GET".to_owned()),
            Some(literals),
        );
        assert_eq!(message(&good), None);
    }

    #[test]
    fn a_constant_passes_a_check_the_compiler_knows() {
        let known = |check: &str| Some(Refinement::Known(check.to_owned()));
        let email = |s: &str| call("Email$parse", Constant::Text(s.to_owned()), known("email"));
        assert_eq!(
            message(&email("nobody")).as_deref(),
            Some("this constant fails the check of `Email`, so `Email.parse` of it always fails")
        );
        assert_eq!(message(&email("someone@example.com")), None);
        assert!(
            message(&call(
                "Name$parse",
                Constant::Text(String::new()),
                known("non_empty")
            ))
            .is_some()
        );
        assert!(
            message(&call(
                "Name$parse",
                Constant::Text("Ada".to_owned()),
                known("non_empty")
            ))
            .is_none()
        );
        assert!(
            message(&call(
                "Port$parse",
                Constant::Number(0.0),
                known("positive")
            ))
            .is_some()
        );
        assert!(
            message(&call(
                "Port$parse",
                Constant::Number(8080.0),
                known("positive")
            ))
            .is_none()
        );
        // a constant of another kind is the types' business, not this check's
        assert!(
            message(&call(
                "Port$parse",
                Constant::Text("x".to_owned()),
                known("positive")
            ))
            .is_none()
        );
    }

    #[test]
    fn the_verdict_of_the_compiler_decides_a_predicate_of_the_module() {
        let fails = call(
            "Even$parse",
            Constant::Number(3.0),
            Some(Refinement::Holds(false)),
        );
        assert!(message(&fails).is_some());
        let holds = call(
            "Even$parse",
            Constant::Number(4.0),
            Some(Refinement::Holds(true)),
        );
        assert_eq!(message(&holds), None);
        // a check the compiler cannot fold is left to run time
        let unknown = call("Even$parse", Constant::Number(3.0), None);
        assert_eq!(message(&unknown), None);
    }
}
