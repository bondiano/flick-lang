//! Arena's semantic oracle: explicit region-owned slots, not an RC proof.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::rc::Rc;

use flick_model::check::{Checker, check_program};
use flick_model::eval::{Machine, Outcome, Program};
use flick_model::native;
use flick_model::run;
use flick_model::syntax::{Const, Lit, Path as Global, RegionOp, Term};
use flick_model::types::{Caps, Kind, Row, Scheme, Stage, Type, Var};

fn var(name: &str) -> Term {
    Term::Var(name.into())
}

fn let_(name: &str, value: Term, body: Term) -> Term {
    Term::Let(name.into(), None, Rc::new(value), Rc::new(body))
}

fn op(name: &str, operation: RegionOp, args: impl IntoIterator<Item = Term>) -> Term {
    Term::apps(
        Term::konst(Const::Region(Global::new("Arena", name), operation)),
        args,
    )
}

fn make() -> Term {
    op("make", RegionOp::ArenaMake, [Term::unit()])
}

fn alloc(value: Term) -> Term {
    op("alloc", RegionOp::ArenaAlloc, [var("arena"), value])
}

fn get(view: Term) -> Term {
    op("get", RegionOp::ArenaGet, [view])
}

fn scope(body: Term) -> Term {
    Term::Scope("s".into(), Rc::new(body))
}

fn with_view(body: Term) -> Term {
    let_(
        "arena",
        make(),
        let_("view", alloc(Term::string("kept")), body),
    )
}

fn say(value: Term) -> Term {
    Term::app(
        Term::konst(Const::Native(
            native::find("flick:rt", "stdoutLine").expect("stdout"),
        )),
        value,
    )
}

fn finalizer(body: Term) -> Term {
    op(
        "add_finalizer",
        RegionOp::AddFinalizer,
        [Term::Task(Rc::new(body))],
    )
}

#[test]
fn slots_are_cleared_at_scope_close_and_copied_values_survive() {
    let program = Program::default();
    let mut machine = Machine::new(&program, 10_000);
    let term = let_("copy", scope(with_view(get(var("view")))), var("copy"));
    let mut saw_live_slots = false;
    let mut saw_closed_scope = false;
    let outcome = machine.run_watched(term, &mut |_, lambda, _| {
        if lambda.arenas.values().any(|arena| !arena.slots.is_empty()) {
            saw_live_slots = true;
        }
        if lambda.regions.first().is_some_and(|region| !region.live) {
            saw_closed_scope = true;
            assert!(lambda.arenas.values().all(|arena| arena.slots.is_empty()));
        }
    });
    assert!(saw_live_slots);
    assert!(
        saw_closed_scope,
        "must observe close before caller continuation"
    );
    assert!(matches!(outcome, Outcome::Value(Term::Lit(Lit::String(s))) if &*s == "kept"));
}

#[test]
fn retained_unused_view_is_safe_but_a_read_after_close_is_rejected() {
    let program = Program::default();
    let held = scope(with_view(var("view")));
    let mut machine = Machine::new(&program, 10_000);
    assert!(matches!(
        machine.run(let_("held", held.clone(), Term::unit())),
        Outcome::Value(_)
    ));
    let mut machine = Machine::new(&program, 10_000);
    let outcome = machine.run(let_("held", held, get(var("held"))));
    assert!(
        matches!(outcome, Outcome::Stuck(why) if why.starts_with("use after close of arena view"))
    );
}

#[test]
fn earlier_and_later_finalizers_read_until_all_finalizers_finish() {
    let cell = Term::app(
        Term::konst(Const::RefMake),
        Term::Task(Rc::new(Term::unit())),
    );
    let read_cell = Term::Await(Rc::new(Term::app(Term::konst(Const::RefGet), var("cell"))));
    let install = Term::apps(
        Term::konst(Const::RefSet),
        [var("cell"), Term::Task(Rc::new(say(get(var("view")))))],
    );
    let body = let_(
        "_earlier",
        finalizer(read_cell),
        with_view(let_(
            "_install",
            install,
            let_("_later", finalizer(say(get(var("view")))), Term::unit()),
        )),
    );
    let program = Program::default();
    let mut machine = Machine::new(&program, 10_000);
    assert!(matches!(
        machine.run(let_("cell", cell, scope(body))),
        Outcome::Value(_)
    ));
    assert_eq!(machine.stdout, "kept\nkept\n");
}

#[test]
fn failed_body_still_releases_slots_after_its_finalizers() {
    let raised = Term::app(Term::konst(Const::Raise), Term::string("failure"));
    let body = with_view(let_("_fin", finalizer(say(get(var("view")))), raised));
    let program = Program::default();
    let mut machine = Machine::new(&program, 10_000);
    let mut saw_closed = false;
    let outcome = machine.run_watched(scope(body), &mut |_, lambda, _| {
        if lambda.regions.first().is_some_and(|region| !region.live) {
            saw_closed = true;
            assert!(lambda.arenas.values().all(|arena| arena.slots.is_empty()));
        }
    });
    assert!(saw_closed);
    assert!(matches!(outcome, Outcome::Raise(Term::Lit(Lit::String(s))) if &*s == "failure"));
    assert_eq!(machine.stdout, "kept\n");
}

#[test]
fn arenas_created_during_finalization_are_retired_with_the_same_owner() {
    let body = let_(
        "_fin",
        finalizer(with_view(say(get(var("view"))))),
        with_view(Term::unit()),
    );
    let program = Program::default();
    let mut machine = Machine::new(&program, 10_000);
    let mut saw_closed = false;
    let outcome = machine.run_watched(scope(body), &mut |_, lambda, _| {
        if lambda.regions.first().is_some_and(|region| !region.live) {
            saw_closed = true;
            assert_eq!(lambda.arenas.len(), 2);
            assert!(lambda.arenas.values().all(|arena| arena.slots.is_empty()));
        }
    });
    assert!(saw_closed);
    assert!(matches!(outcome, Outcome::Value(_)));
    assert_eq!(machine.stdout, "kept\n");
}

#[test]
fn closing_an_inner_scope_keeps_the_outer_arena_live() {
    let inner = Term::Scope("t".into(), Rc::new(with_view(get(var("view")))));
    let body = with_view(let_(
        "inner",
        inner,
        Term::Tuple([get(var("view")), var("inner")].into()),
    ));
    let program = Program::default();
    let mut machine = Machine::new(&program, 10_000);
    let mut saw_inner_close = false;
    let outcome = machine.run_watched(scope(body), &mut |_, lambda, _| {
        if lambda.regions.get(1).is_some_and(|region| !region.live) && lambda.regions[0].live {
            saw_inner_close = true;
            for arena in lambda.arenas.values() {
                assert_eq!(arena.slots.len(), usize::from(arena.owner == 0));
            }
        }
    });
    assert!(saw_inner_close);
    assert!(
        matches!(outcome, Outcome::Value(Term::Tuple(values)) if values.iter().all(|value| matches!(value, Term::Lit(Lit::String(s)) if &**s == "kept")))
    );
}

#[test]
fn arena_sources_run_in_the_semantic_model() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run/runtime");
    for case in ["arena_scope", "arena_finalizers"] {
        let modules = run::compile(&root.join(format!("{case}.fl")))
            .unwrap_or_else(|error| panic!("{case}: {error}"));
        let program = run::program(&modules);
        let errors = run::check(&modules, &program);
        assert!(errors.is_empty(), "{case}: {errors:?}");
        let main = run::main_of(&modules).expect("main");
        let ran = run::run_main(&program, &main, 100_000);
        assert!(
            matches!(ran.outcome, Outcome::Value(_)),
            "{case}: {:?}",
            ran.outcome
        );
        let expected = fs::read_to_string(root.join(format!("{case}.stdout")))
            .expect("independent corpus expectation");
        assert_eq!(ran.stdout, expected, "{case}");
    }
}

#[test]
fn arena_foreign_declarations_check_against_their_polymorphic_schemes() {
    let program = run::program(&[]);
    let declarations = run::decls(&[]);
    for name in ["make", "alloc", "get"] {
        let path = Global::new("Arena", name);
        let term = &program.globals[&path];
        let scheme = &declarations.values[&path];
        Checker::new(&declarations)
            .check_top(term, scheme)
            .unwrap_or_else(|error| panic!("standalone {path}: {error}"));
    }
    let modules = HashSet::from(["Arena".into()]);
    let errors = check_program(&program, &declarations, &modules);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn polymorphic_arena_intrinsics_do_not_allow_a_read_after_region_close() {
    let declarations = run::decls(&[]);
    let signature = Scheme::new(
        vec![],
        Type::fun(
            Type::con("Prelude", "Unit"),
            Stage::Runtime(Row::none(), Row::none()),
            Type::con("Prelude", "String"),
        ),
    );
    let escaped = Term::lam(
        "_",
        let_("view", scope(with_view(var("view"))), get(var("view"))),
    );
    assert!(
        Checker::new(&declarations)
            .check_top(&escaped, &signature)
            .is_err()
    );
}

#[test]
fn arena_get_preserves_the_use_qualifier_of_a_captured_callback() {
    let declarations = run::decls(&[]);
    let runtime = || Stage::Runtime(Row::none(), Row::none());
    let callback = Type::fun(
        Type::con("Prelude", "Unit"),
        runtime(),
        Type::con("Prelude", "String"),
    );
    let captured = Type::Captured(
        Rc::new(callback.clone()),
        Caps::atom(Var::Named("s".into())),
    );
    let view = Type::app(
        Type::app(
            Type::con("Arena", "Value"),
            Type::Var(Var::Named("t".into())),
        ),
        captured.clone(),
    );
    let signature = |result| {
        Scheme::new(
            vec![("s".into(), Kind::Region), ("t".into(), Kind::Region)],
            Type::fun(view.clone(), runtime(), result),
        )
    };
    let read = Term::lam("view", get(var("view")));
    assert!(
        Checker::new(&declarations)
            .check_top(&read, &signature(captured))
            .is_ok()
    );
    // A function result inherits earlier parameters' reach by elision.
    // A separate local signature has no such parameter and must not erase
    // the returned callback's region-dependent use.
    let erased = Scheme::new(vec![], Type::Captured(Rc::new(callback), Caps::default()));
    let narrow = Term::lam(
        "view",
        Term::Let(
            "erased".into(),
            Some(Rc::new(erased)),
            Rc::new(get(var("view"))),
            Rc::new(Term::unit()),
        ),
    );
    assert!(
        Checker::new(&declarations)
            .check_top(&narrow, &signature(Type::con("Prelude", "Unit")))
            .is_err()
    );
}
