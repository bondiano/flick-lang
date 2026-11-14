//! Iterative checked-pattern layout and successful-path binder validation.
use std::collections::BTreeSet;

use crate::Pat;

use super::Error;

enum Work<'a> {
    Visit(&'a Pat),
    Join(usize),
    Alternatives(usize),
}

/// Returns the common successful-path binders in lexical order.
/// Or alternatives may bind names in different positions, but their sets
/// must agree. A name repeated along one successful path is invalid.
pub fn pattern_bindings(pattern: &Pat) -> Result<Vec<String>, Error> {
    let mut work = vec![Work::Visit(pattern)];
    let mut sets = Vec::<BTreeSet<String>>::new();
    while let Some(next) = work.pop() {
        match next {
            Work::Visit(pattern) => visit(pattern, &mut work, &mut sets)?,
            Work::Join(count) => {
                let mut joined = BTreeSet::new();
                for names in sets.drain(sets.len() - count..) {
                    for name in names {
                        if !joined.insert(name.clone()) {
                            return Err(Error::DuplicatePatternBinding(name));
                        }
                    }
                }
                sets.push(joined);
            }
            Work::Alternatives(count) => {
                let start = sets.len() - count;
                let first = &sets[start];
                if sets[start + 1..].iter().any(|names| names != first) {
                    return Err(Error::UnequalOrBindings);
                }
                let common = sets.swap_remove(start);
                sets.truncate(start);
                sets.push(common);
            }
        }
    }
    Ok(sets
        .pop()
        .expect("one validated root")
        .into_iter()
        .collect())
}

fn visit<'a>(
    pattern: &'a Pat,
    work: &mut Vec<Work<'a>>,
    sets: &mut Vec<BTreeSet<String>>,
) -> Result<(), Error> {
    let children = match pattern {
        Pat::Wild | Pat::Lit(_) => {
            sets.push(BTreeSet::new());
            return Ok(());
        }
        Pat::Bind(name) => {
            sets.push(BTreeSet::from([name.clone()]));
            return Ok(());
        }
        Pat::Tuple(items) => items.iter().collect::<Vec<_>>(),
        Pat::List(items, rest) => items.iter().chain(rest.iter().map(Box::as_ref)).collect(),
        Pat::Record(fields) => {
            let mut labels = BTreeSet::new();
            for (label, _) in fields {
                if !labels.insert(label) {
                    return Err(Error::DuplicatePatternField(label.clone()));
                }
            }
            fields.iter().map(|(_, pattern)| pattern).collect()
        }
        Pat::Con(constructor, items) => {
            let expected = constructor
                .arity
                .checked_add(constructor.dicts)
                .ok_or(Error::PatternArityOverflow)?;
            if items.len() != expected {
                return Err(Error::ConstructorPatternArity {
                    expected,
                    actual: items.len(),
                });
            }
            items.iter().collect()
        }
        Pat::Or(items) => {
            if items.is_empty() {
                return Err(Error::EmptyOrPattern);
            }
            work.push(Work::Alternatives(items.len()));
            work.extend(items.iter().rev().map(Work::Visit));
            return Ok(());
        }
    };
    work.push(Work::Join(children.len()));
    work.extend(children.into_iter().rev().map(Work::Visit));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Constructor, Lit, Rep};

    fn bind(name: &str) -> Pat {
        Pat::Bind(name.into())
    }
    fn constructor(arity: usize, dicts: usize) -> Constructor {
        Constructor {
            module: "M".into(),
            ty: "Box".into(),
            name: "Box".into(),
            arity,
            dicts,
            rep: Rep::Tagged,
        }
    }

    #[test]
    fn alternatives_use_common_sorted_names_not_first_branch_positions() {
        let pattern = Pat::Or(vec![
            Pat::Tuple(vec![bind("z"), bind("a")]),
            Pat::Tuple(vec![bind("a"), bind("z")]),
        ]);
        assert_eq!(pattern_bindings(&pattern), Ok(vec!["a".into(), "z".into()]));
    }
    #[test]
    fn invalid_patterns_are_rejected_without_running_a_branch() {
        for (pattern, expected) in [
            (
                Pat::Tuple(vec![bind("x"), bind("x")]),
                Error::DuplicatePatternBinding("x".into()),
            ),
            (Pat::Or(vec![]), Error::EmptyOrPattern),
            (
                Pat::Or(vec![bind("x"), Pat::Wild]),
                Error::UnequalOrBindings,
            ),
            (
                Pat::Record(vec![("x".into(), Pat::Wild), ("x".into(), Pat::Wild)]),
                Error::DuplicatePatternField("x".into()),
            ),
            (
                Pat::Con(constructor(1, 1), vec![Pat::Wild]),
                Error::ConstructorPatternArity {
                    expected: 2,
                    actual: 1,
                },
            ),
            (
                Pat::Con(constructor(usize::MAX, 1), vec![]),
                Error::PatternArityOverflow,
            ),
            (
                Pat::Tuple(vec![bind("x"), Pat::Or(vec![bind("x"), bind("x")])]),
                Error::DuplicatePatternBinding("x".into()),
            ),
        ] {
            assert_eq!(pattern_bindings(&pattern), Err(expected));
        }
        assert_eq!(
            pattern_bindings(&Pat::Con(constructor(1, 1), vec![Pat::Wild, bind("dict")])),
            Ok(vec!["dict".into()])
        );
    }
    #[test]
    fn pattern_validation_uses_an_explicit_stack() {
        let mut pattern = Pat::Lit(Lit::Unit);
        for _ in 0..10_000 {
            pattern = Pat::List(vec![], Some(Box::new(pattern)));
        }
        assert_eq!(pattern_bindings(&pattern), Ok(vec![]));
        // The derived Pat destructor is not part of validator traversal.
        while let Pat::List(_, Some(rest)) = pattern {
            pattern = *rest;
        }
    }
}
