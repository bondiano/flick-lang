//! Translation preserves union laws independently of operand order.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use flick_core::types as fc;

use super::row;
use crate::syntax::Path;
use crate::types::{Row, Type, Var};

#[test]
fn unrestricted_row_union_clears_exclusions_in_both_orders() {
    let variable = fc::Type::Var("e".into());
    let label = Path::new("Test", "Missing");
    let excluded = fc::Type::Without {
        row: Box::new(variable.clone()),
        gone: vec![fc::Elem::named("Test.Missing")],
    };
    let var = Var::Named("e".into());
    assert_eq!(
        row(&excluded).less,
        BTreeSet::from([(var.clone(), label.clone())])
    );
    let named = fc::Type::Row(fc::Row {
        entries: vec![(
            fc::Elem::named("Test.Missing"),
            fc::Type::app(
                fc::Type::con("Test", "Missing"),
                fc::Type::con("Prelude", "Int"),
            ),
        )],
        tail: Some(Box::new(excluded.clone())),
        ..fc::Row::default()
    });
    for (restricted, expected_elems) in [
        (excluded, BTreeMap::new()),
        (
            named,
            [(label, Rc::from([Type::con("Prelude", "Int")]))].into(),
        ),
    ] {
        let expected = Row {
            elems: expected_elems,
            ..Row::var(var.clone())
        };
        for (left, right) in [
            (restricted.clone(), variable.clone()),
            (variable.clone(), restricted.clone()),
        ] {
            assert_eq!(
                row(&fc::Type::Union(Box::new(left), Box::new(right))),
                expected
            );
        }
    }
}
