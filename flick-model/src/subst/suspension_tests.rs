#[cfg(test)]
mod suspension_tests {
    use super::{free_vars, inline_suspension};
    use crate::check::{Checker, scope_service};
    use crate::decls::Decls;
    use crate::eval::{Machine, Outcome, Program};
    use crate::syntax::{Arm, Con, Head, Lit, Name, Pat, Path, Term};
    use crate::types::{Fields, Kind, Row, Scheme, Stage, Type, Var};
    use std::rc::Rc;

    fn inline(value: &Term, body: &Term) -> Option<Term> {
        let mut next = 0;
        inline_suspension(&Name::from("operand"), value, body, &mut || {
            next += 1;
            format!("$fresh{next}").into()
        })
    }

    #[test]
    fn inert_aggregate_values_commute_without_capturing_their_suspensions() {
        let suspended = Term::lam("arg", Term::Var("captured".into()));
        let record = Term::Record(Rc::from([
            ("lookup".into(), suspended.clone()),
            ("capacity".into(), Term::Lit(Lit::Int(4))),
        ]));
        let values = [
            record,
            Term::Tuple(Rc::from([suspended.clone(), Term::unit()])),
            Term::List(Rc::from([suspended])),
        ];
        for value in values {
            let body = Term::lam("captured", Term::Var("operand".into()));
            let result = inline(&value, &body).expect("single-use inert aggregate");
            assert!(free_vars(&result).contains(&Name::from("captured")));
            assert!(matches!(result, Term::Lam(name, _) if &*name != "captured"));
        }
    }

    #[test]
    fn constructor_apply_keeps_its_closed_value_invariant() {
        let constructor = Rc::new(Con {
            ty: Path::new("Test", "Box"),
            name: "Box".into(),
            arity: 1,
            dicts: 0,
        });
        let value = |child| Term::Apply(Head::Con(constructor.clone()), Rc::from([child]));
        let open = value(Term::lam("arg", Term::Var("captured".into())));
        assert!(
            inline(&open, &Term::Var("operand".into())).is_none(),
            "an open Apply is not a valid closed constructor value"
        );
        let closed = value(Term::lam("arg", Term::Lit(Lit::Int(7))));
        let body = Term::Match(
            Rc::new(Term::Var("operand".into())),
            Rc::from([Arm {
                pat: Pat::Con(constructor.clone(), Rc::from([Pat::Var("method".into())])),
                guard: None,
                body: Term::app(Term::Var("method".into()), Term::unit()),
            }]),
        );
        let normalized = inline(&closed, &body).expect("closed inert constructor");
        let program = Program::default();
        assert!(matches!(
            Machine::new(&program, 100).run(normalized),
            Outcome::Value(Term::Lit(Lit::Int(7)))
        ));
        let source = Term::app(
            Term::Apply(Head::Con(constructor), Rc::from([])),
            Term::lam("arg", Term::Var("captured".into())),
        );
        assert!(
            inline(&source, &body).is_none(),
            "source constructor App spines keep their existing ordered path"
        );
    }

    #[test]
    fn effectful_children_and_shared_aggregates_keep_their_let_and_order() {
        let body = Term::Let(
            "later".into(),
            None,
            Rc::new(Term::Stuck("later computation".into())),
            Rc::new(Term::Var("operand".into())),
        );
        for value in [
            Term::Record(Rc::from([(
                "field".into(),
                Term::Stuck("earlier field".into()),
            )])),
            Term::Tuple(Rc::from([Term::Stuck("earlier field".into())])),
            Term::List(Rc::from([Term::Stuck("earlier field".into())])),
        ] {
            assert!(
                inline(&value, &body).is_none(),
                "a child computation must not move"
            );
            let original = Term::Let(
                "operand".into(),
                None,
                Rc::new(value),
                Rc::new(body.clone()),
            );
            let program = Program::default();
            assert!(
                matches!(Machine::new(&program, 100).run(original), Outcome::Stuck(message) if message == "earlier field")
            );
        }
        for child in [
            Term::Global(Path::new("Test", "lookup")),
            Term::Field(Rc::new(Term::Var("object".into())), "getter".into()),
            Term::app(Term::Var("read_cell".into()), Term::unit()),
            Term::Loc(0),
        ] {
            let value = Term::Record(Rc::from([("field".into(), child)]));
            assert!(
                inline(&value, &body).is_none(),
                "a lookup or cell is not an inert field"
            );
        }
        let shared = Term::Record(Rc::from([(
            "lookup".into(),
            Term::lam("arg", Term::unit()),
        )]));
        let twice = Term::Tuple(Rc::from([
            Term::Var("operand".into()),
            Term::Var("operand".into()),
        ]));
        assert!(
            inline(&shared, &twice).is_none(),
            "shared aggregates retain identity and sharing"
        );
    }

    #[test]
    fn moving_an_open_task_does_not_capture_a_shadowing_lambda() {
        let value = Term::Task(Rc::new(Term::Var("captured".into())));
        let body = Term::lam("captured", Term::Var("operand".into()));
        let result = inline(&value, &body).expect("one task use");
        assert!(free_vars(&result).contains(&Name::from("captured")));
        assert!(matches!(result, Term::Lam(name, _) if &*name != "captured"));
    }

    #[test]
    fn moving_a_lambda_preserves_pattern_guard_and_body_bindings() {
        let value = Term::lam("arg", Term::Var("captured".into()));
        let body = Term::Match(
            Rc::new(Term::unit()),
            Rc::from([Arm {
                pat: Pat::Var("captured".into()),
                guard: Some(Term::Var("captured".into())),
                body: Term::Var("operand".into()),
            }]),
        );
        let result = inline(&value, &body).expect("one lambda use");
        assert!(free_vars(&result).contains(&Name::from("captured")));
        let Term::Match(_, arms) = result else {
            panic!("match retained")
        };
        assert!(matches!(&arms[0].pat, Pat::Var(name) if &**name != "captured"));
        let Pat::Var(name) = &arms[0].pat else {
            panic!("variable pattern")
        };
        assert!(matches!(&arms[0].guard, Some(Term::Var(found)) if found == name));
    }

    #[test]
    fn multiple_uses_and_computations_keep_their_original_let() {
        let twice = Term::Tuple(Rc::from([
            Term::Var("operand".into()),
            Term::Var("operand".into()),
        ]));
        assert!(inline(&Term::Task(Rc::new(Term::unit())), &twice).is_none());
        let call = Term::app(Term::Var("effectful".into()), Term::unit());
        assert!(inline(&call, &Term::Var("operand".into())).is_none());
    }

    #[test]
    fn task_bodies_remain_suspended_and_annotated_bindings_remain_present() {
        let signature = Rc::new(Scheme::new(Vec::new(), Type::Var(Var::Named("a".into()))));
        let value = Term::Task(Rc::new(Term::Let(
            "local".into(),
            Some(signature.clone()),
            Rc::new(Term::Var("input".into())),
            Rc::new(Term::Var("local".into())),
        )));
        let result = inline(&value, &Term::Var("operand".into())).expect("one task use");
        let Term::Task(body) = result else {
            panic!("task retained")
        };
        assert!(matches!(&*body, Term::Let(_, Some(actual), _, _) if actual == &signature));
    }

    #[test]
    fn scope_freshening_renames_its_signatures_without_capturing_the_moved_task() {
        let signature = Rc::new(Scheme::new(
            Vec::new(),
            Type::Var(Var::Named("region".into())),
        ));
        let value = Term::Task(Rc::new(Term::Var("region".into())));
        let body = Term::Scope(
            "region".into(),
            Rc::new(Term::Let(
                "local".into(),
                Some(signature),
                Rc::new(Term::unit()),
                Rc::new(Term::Var("operand".into())),
            )),
        );
        let result = inline(&value, &body).expect("single suspension");
        assert!(free_vars(&result).contains(&Name::from("region")));
        let Term::Scope(region, body) = result else {
            panic!("scope retained")
        };
        assert_ne!(&*region, "region");
        let Term::Let(_, Some(signature), _, _) = &*body else {
            panic!("signature retained")
        };
        assert_eq!(signature.body, Type::Var(Var::Named(region)));
    }

    #[test]
    fn commuting_a_task_does_not_run_it_before_an_earlier_computation() {
        for aggregate in [false, true] {
            let suspended = Term::Task(Rc::new(Term::Stuck("task body".into())));
            let value = if aggregate {
                Term::Record(Rc::from([("lookup".into(), suspended)]))
            } else {
                suspended
            };
            let operand = if aggregate {
                Term::Field(Rc::new(Term::Var("operand".into())), "lookup".into())
            } else {
                Term::Var("operand".into())
            };
            let body = Term::Let(
                "earlier".into(),
                None,
                Rc::new(Term::Stuck("earlier computation".into())),
                Rc::new(Term::Await(Rc::new(operand))),
            );
            let original = Term::Let(
                "operand".into(),
                None,
                Rc::new(value.clone()),
                Rc::new(body.clone()),
            );
            let normalized = inline(&value, &body).expect("one task use");
            for term in [original, normalized] {
                let program = Program::default();
                assert!(
                    matches!(Machine::new(&program, 100).run(term), Outcome::Stuck(message) if message.contains("earlier computation"))
                );
            }
        }
    }

    #[test]
    fn normalized_rank_two_task_still_checks_its_result_type() {
        let unit = Type::con("Prelude", "Unit");
        let task = Type::Eff(Row::none(), Row::none(), Rc::new(unit.clone()));
        let parameter = Type::Forall(Rc::new(Scheme::new(vec![("t".into(), Kind::Region)], task)));
        let signature = Rc::new(Scheme::new(
            Vec::new(),
            Type::fun(parameter, Stage::Pure, unit.clone()),
        ));
        for (body, valid) in [(Term::unit(), true), (Term::Lit(Lit::Int(7)), false)] {
            let argument = Term::Task(Rc::new(body));
            let use_site = Term::Let(
                "consume".into(),
                Some(signature.clone()),
                Rc::new(Term::lam("ignored", Term::unit())),
                Rc::new(Term::app(
                    Term::Var("consume".into()),
                    Term::Var("operand".into()),
                )),
            );
            let normalized = inline(&argument, &use_site).expect("single task use");
            let decls = Decls::default();
            let checked =
                Checker::new(&decls).check_top(&normalized, &Scheme::new(Vec::new(), unit.clone()));
            assert_eq!(checked.is_ok(), valid, "rank-two task result: {checked:?}");
        }
    }

    #[test]
    fn normalization_cannot_make_a_fixed_owner_task_rank_two() {
        fixed_owner_stays_fixed(false);
    }

    #[test]
    fn normalization_cannot_make_a_fixed_owner_record_field_rank_two() {
        fixed_owner_stays_fixed(true);
    }

    fn fixed_owner_stays_fixed(record: bool) {
        let unit = Type::con("Prelude", "Unit");
        let scope = |region: &str| {
            let mut row = Row::none();
            row.elems.insert(
                scope_service(),
                Rc::from([Type::Var(Var::Named(region.into()))]),
            );
            row
        };
        let parameter = Type::Forall(Rc::new(Scheme::new(
            vec![("t".into(), Kind::Region)],
            if record {
                Type::fun(
                    unit.clone(),
                    Stage::Runtime(scope("t"), Row::none()),
                    unit.clone(),
                )
            } else {
                Type::Eff(scope("t"), Row::none(), Rc::new(unit.clone()))
            },
        )));
        let parameter = if record {
            Type::Record(Fields {
                fields: [("lookup".into(), parameter)].into_iter().collect(),
                tail: None,
            })
        } else {
            parameter
        };
        let consume = Rc::new(Scheme::new(
            Vec::new(),
            Type::fun(parameter, Stage::Pure, unit.clone()),
        ));
        let acquire = Rc::new(Scheme::new(
            Vec::new(),
            Type::fun(
                unit.clone(),
                Stage::Runtime(scope("s"), Row::none()),
                unit.clone(),
            ),
        ));
        let call = Term::app(Term::Var("acquire".into()), Term::unit());
        let value = if record {
            Term::Record(Rc::from([("lookup".into(), Term::lam("argument", call))]))
        } else {
            Term::Task(Rc::new(call))
        };
        let use_site = Term::Let(
            "consume".into(),
            Some(consume),
            Rc::new(Term::lam("ignored", Term::unit())),
            Rc::new(Term::app(
                Term::Var("consume".into()),
                Term::Var("operand".into()),
            )),
        );
        let normalized = inline(&value, &use_site).expect("single task use");
        let body = Term::Scope(
            "s".into(),
            Rc::new(Term::Let(
                "acquire".into(),
                Some(acquire),
                Rc::new(Term::lam("ignored", Term::unit())),
                Rc::new(normalized),
            )),
        );
        let term = Term::lam("input", body);
        let signature = Scheme::new(
            Vec::new(),
            Type::fun(unit.clone(), Stage::Runtime(Row::none(), Row::none()), unit),
        );
        let decls = Decls::default();
        let error = Checker::new(&decls)
            .check_top(&term, &signature)
            .expect_err("fixed acquisition owner is not universal");
        assert_eq!(error.rule, "≡");
        assert!(
            error.message.contains("`s`") && error.message.contains("`t'"),
            "{error:?}"
        );
    }

    #[test]
    fn mixed_record_hoists_evaluated_fields_in_order_and_inlines_suspensions() {
        let value = Term::Record(Rc::from([
            ("acquire".into(), Term::Task(Rc::new(Term::unit()))),
            ("first".into(), Term::Stuck("first field".into())),
            ("second".into(), Term::Stuck("second field".into())),
        ]));
        let body = Term::Let(
            "later".into(),
            None,
            Rc::new(Term::Stuck("later computation".into())),
            Rc::new(Term::Var("operand".into())),
        );
        let normalized = inline(&value, &body).expect("record with suspension field");
        let program = Program::default();
        assert!(
            matches!(Machine::new(&program, 100).run(normalized), Outcome::Stuck(message) if message == "first field"),
            "evaluated fields stay before body in left-to-right order"
        );
    }
}
