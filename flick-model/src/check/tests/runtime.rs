//! Runtime configurations retain the arguments of service implementations.

use std::rc::Rc;

use super::{a, decls, int, poly, record, rejected_by, rt, task, ty};
use crate::check::{Checker, check_config};
use crate::eval::{Lambda, Machine, Outcome, Program};
use crate::syntax::{Path, Term};
use crate::types::{Row, Type};

/// Runtime bindings retain the instance inferred from an implementation.
#[test]
fn runtime_bindings_retain_service_arguments() {
    let mut d = decls();
    let store = Path::new(super::T, "Store");
    d.service_params.insert(store.clone(), 1);
    d.services.insert(
        store.clone(),
        vec![("get".into(), poly(Type::fun(ty("Unit"), rt(), a())))],
    );
    let implementation = record(&[("get", Term::lam("u", int(42)))]);
    let expected = Type::Env(Row {
        elems: [(store.clone(), Rc::from([ty("Int")]))].into(),
        ..Row::default()
    });
    let wrong = Type::Env(Row {
        elems: [(store.clone(), Rc::from([ty("String")]))].into(),
        ..Row::default()
    });
    for term in [
        Term::Bind(store.clone(), Rc::new(implementation.clone())),
        Term::Bind(
            store.clone(),
            Rc::new(Term::Await(Rc::new(task(implementation.clone())))),
        ),
        Term::Env(Rc::new([(store, implementation)].into())),
    ] {
        let mut checker = Checker::new(&d);
        checker.config = true;
        let (actual, _) = checker
            .infer_top(&term)
            .expect("a Store Int implementation");
        assert_eq!(actual, expected);
        assert!(check_config(&d, &[], &Lambda::default(), &term, &expected).is_ok());
        rejected_by(
            check_config(&d, &[], &Lambda::default(), &term, &wrong),
            "≡",
        );
        let program = Program::default();
        let mut machine = Machine::new(&program, 100);
        let outcome = machine.run_watched(term, &mut |mem, regions, term| {
            check_config(&d, mem, regions, term, &expected)
                .expect("every runtime binding step retains Store Int");
        });
        assert!(
            matches!(outcome, Outcome::Value(Term::Env(_))),
            "{outcome:?}"
        );
    }
}
