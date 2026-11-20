//! Closed service and error rows fixing type variables in either order (#762):
//! the reference model and `flick-core` agree on plain and value-bounded
//! polymorphic calls, branches, lists, tasks, unannotated cells, and rejection
//! of written closed rows and invariant `Ref`s.

use std::rc::Rc;

use flick_model::eval::Outcome;
use flick_model::generate;
use flick_model::syntax::{Arm, Con, Const, Lit, Pat, Path, Term};
use flick_model::types::{Caps, Kind, Row, Scheme, Stage, Type, Var};

use crate::live_support::{core, probe, v, verdict};

fn service_order_program(stage: Stage, body: Term) -> generate::Program {
    let unit = || Type::con("Prelude", "Unit");
    let none = || Stage::Runtime(Row::none(), Row::none());
    let named = |x: &str| Var::Named(x.into());
    let list = |t: Type| Type::app(Type::con("Prelude", "List"), t);
    let plain_keep2 = Scheme::new(
        vec![("a".into(), Kind::Type)],
        Type::fun(
            list(Type::Var(named("a"))),
            Stage::Pure,
            Type::fun(Type::Var(named("a")), none(), list(Type::Var(named("a")))),
        ),
    );
    let bounded_a = || Type::Bounded(Rc::new(Type::Var(named("a"))), Caps::var(named("c")));
    let bounded_keep2 = Scheme::new(
        vec![("a".into(), Kind::Type), ("c".into(), Kind::Captures)],
        Type::fun(
            list(bounded_a()),
            Stage::Pure,
            Type::fun(bounded_a(), none(), list(bounded_a())),
        ),
    );
    let quiet_scheme = Scheme::new(Vec::new(), Type::fun(unit(), none(), unit()));
    let ready_scheme = Scheme::new(
        Vec::new(),
        Type::Eff(Row::none(), Row::none(), Rc::new(unit())),
    );
    let returns_x = Term::lam("xs", Term::lam("x", Term::List(Rc::from([v("x")]))));
    let inner = Term::Let(
        "ready".into(),
        Some(Rc::new(ready_scheme)),
        Rc::new(Term::Task(Rc::new(Term::unit()))),
        Rc::new(body),
    );
    let inner = Term::Let(
        "quiet".into(),
        Some(Rc::new(quiet_scheme)),
        Rc::new(Term::lam("u", Term::unit())),
        Rc::new(inner),
    );
    let inner = Term::Let(
        "keep2_bounded".into(),
        Some(Rc::new(bounded_keep2)),
        Rc::new(returns_x.clone()),
        Rc::new(inner),
    );
    let inner = Term::Let(
        "keep2".into(),
        Some(Rc::new(plain_keep2)),
        Rc::new(returns_x),
        Rc::new(inner),
    );
    generate::Program {
        scheme: Scheme::new(Vec::new(), Type::fun(unit(), stage, unit())),
        term: Term::lam("u", inner),
    }
}

fn log_call_term() -> Term {
    let ask = Term::app(
        Term::konst(Const::Ask(Path::new("Fixture", "Log"))),
        Term::unit(),
    );
    Term::app(Term::Field(Rc::new(ask), "log".into()), Term::string("x"))
}

fn log_stage() -> Stage {
    let mut r = Row::none();
    r.elems.insert(Path::new("Fixture", "Log"), Rc::from([]));
    Stage::Runtime(r, Row::none())
}

fn bool_branch(then_e: Term, else_e: Term) -> Term {
    let arms = [
        Arm {
            pat: Pat::Lit(Lit::Bool(true)),
            guard: None,
            body: then_e,
        },
        Arm {
            pat: Pat::Wild,
            guard: None,
            body: else_e,
        },
    ];
    Term::Match(Rc::new(Term::Lit(Lit::Bool(true))), Rc::from(arms))
}

fn some_con() -> Rc<Con> {
    Rc::new(Con {
        ty: Path::new("Prelude", "Option"),
        name: "Some".into(),
        arity: 1,
        dicts: 0,
    })
}

fn wrapped_service_order_accepted() -> Vec<(&'static str, generate::Program)> {
    let logging = || Term::lam("u", log_call_term());
    let some = |t: Term| Term::app(Term::konst_con(some_con()), t);
    let one = |t: Term| Term::List(Rc::from([t]));
    let keep = |name: &str, a: Term, b: Term| Term::apps(v(name), [a, b]);
    let let_in = |name: &str, rhs: Term, body: Term| {
        Term::Let(name.into(), None, Rc::new(rhs), Rc::new(body))
    };
    let match_head = |pat: Pat, body: Term, xs: Term| {
        let arms = [
            Arm {
                pat: Pat::List(Rc::from([pat]), Some(Rc::new(Pat::Wild))),
                guard: None,
                body,
            },
            Arm {
                pat: Pat::Wild,
                guard: None,
                body: Term::unit(),
            },
        ];
        Term::Match(Rc::new(xs), Rc::from(arms))
    };
    let call_opt = |xs: Term| {
        match_head(
            Pat::Con(some_con(), Rc::from([Pat::Var("f".into())])),
            Term::app(v("f"), Term::unit()),
            xs,
        )
    };
    let call_pair = |xs: Term| {
        match_head(
            Pat::Tuple(Rc::from([Pat::Var("f".into()), Pat::Wild])),
            Term::app(v("f"), Term::unit()),
            xs,
        )
    };
    let call_rec = |xs: Term| {
        match_head(
            Pat::Var("r".into()),
            Term::app(Term::Field(Rc::new(v("r")), "run".into()), Term::unit()),
            xs,
        )
    };
    let rec_run = |f: Term| Term::Record(Rc::from([("run".into(), f)]));
    let pair = |f: Term| Term::Tuple(Rc::from([f, Term::unit()]));
    let p = |body| service_order_program(log_stage(), body);
    vec![
        (
            "bounded wrapped option quiet_first",
            p(let_in(
                "sl",
                some(logging()),
                call_opt(keep("keep2_bounded", one(some(v("quiet"))), v("sl"))),
            )),
        ),
        (
            "bounded wrapped option closure_first",
            p(let_in(
                "sq",
                some(v("quiet")),
                call_opt(keep("keep2_bounded", one(some(logging())), v("sq"))),
            )),
        ),
        (
            "bounded wrapped tuple quiet_first",
            p(let_in(
                "pl",
                pair(logging()),
                call_pair(keep("keep2_bounded", one(pair(v("quiet"))), v("pl"))),
            )),
        ),
        (
            "bounded wrapped tuple closure_first",
            p(let_in(
                "pq",
                pair(v("quiet")),
                call_pair(keep("keep2_bounded", one(pair(logging())), v("pq"))),
            )),
        ),
        (
            "bounded wrapped record quiet_first",
            p(let_in(
                "rl",
                rec_run(logging()),
                call_rec(keep("keep2_bounded", one(rec_run(v("quiet"))), v("rl"))),
            )),
        ),
        (
            "bounded wrapped record closure_first",
            p(let_in(
                "rq",
                rec_run(v("quiet")),
                call_rec(keep("keep2_bounded", one(rec_run(logging())), v("rq"))),
            )),
        ),
    ]
}

fn service_order_accepted() -> Vec<(&'static str, generate::Program)> {
    let logging = || Term::lam("u", log_call_term());
    let logging_task = || Term::Task(Rc::new(log_call_term()));
    let head_with = |var: &str, body: Term, xs: Term| {
        let arms = [
            Arm {
                pat: Pat::List(Rc::from([Pat::Var(var.into())]), Some(Rc::new(Pat::Wild))),
                guard: None,
                body,
            },
            Arm {
                pat: Pat::Wild,
                guard: None,
                body: Term::unit(),
            },
        ];
        Term::Match(Rc::new(xs), Rc::from(arms))
    };
    let call_head = |xs: Term| head_with("f", Term::app(v("f"), Term::unit()), xs);
    let await_head = |xs: Term| head_with("t", Term::Await(Rc::new(v("t"))), xs);
    let one = |t: Term| Term::List(Rc::from([t]));
    let keep = |name: &str, a: Term, b: Term| Term::apps(v(name), [a, b]);
    let branch_call = |a: Term, b: Term| {
        Term::Let(
            "f".into(),
            None,
            Rc::new(bool_branch(a, b)),
            Rc::new(Term::app(v("f"), Term::unit())),
        )
    };
    let cell_and_call = Term::Let(
        "cell".into(),
        None,
        Rc::new(Term::app(Term::konst(Const::RefMake), v("quiet"))),
        Rc::new(Term::Let(
            "_".into(),
            None,
            Rc::new(Term::apps(
                Term::konst(Const::RefSet),
                [v("cell"), logging()],
            )),
            Rc::new(Term::app(
                Term::app(Term::konst(Const::RefGet), v("cell")),
                Term::unit(),
            )),
        )),
    );
    let p = |body| service_order_program(log_stage(), body);
    let mut cases = vec![
        (
            "plain closure_first",
            p(call_head(keep("keep2", one(logging()), v("quiet")))),
        ),
        (
            "plain closure_last",
            p(call_head(keep("keep2", one(v("quiet")), logging()))),
        ),
        (
            "bounded closure_first",
            p(call_head(keep("keep2_bounded", one(logging()), v("quiet")))),
        ),
        (
            "bounded closure_last",
            p(call_head(keep("keep2_bounded", one(v("quiet")), logging()))),
        ),
        ("branch pure_first", p(branch_call(v("quiet"), logging()))),
        ("branch pure_last", p(branch_call(logging(), v("quiet")))),
        (
            "list quiet_first",
            p(call_head(Term::List(Rc::from([v("quiet"), logging()])))),
        ),
        (
            "list logged_first",
            p(call_head(Term::List(Rc::from([logging(), v("quiet")])))),
        ),
        (
            "task ready_first",
            p(await_head(keep("keep2", one(v("ready")), logging_task()))),
        ),
        (
            "task bounded ready_first",
            p(await_head(keep(
                "keep2_bounded",
                one(v("ready")),
                logging_task(),
            ))),
        ),
        (
            "unannotated ref widened and called with Log",
            p(cell_and_call),
        ),
    ];
    cases.extend(wrapped_service_order_accepted());
    cases
}

fn service_order_rejected() -> Vec<(&'static str, generate::Program)> {
    let unit = || Type::con("Prelude", "Unit");
    let none = || Stage::Runtime(Row::none(), Row::none());
    let logging = || Term::lam("u", log_call_term());
    let quiet_scheme = Scheme::new(Vec::new(), Type::fun(unit(), none(), unit()));
    let closed_cell = Scheme::new(
        Vec::new(),
        Type::Ref(Rc::new(Type::fun(unit(), none(), unit()))),
    );
    let closed_param = Scheme::new(
        Vec::new(),
        Type::fun(Type::fun(unit(), none(), unit()), none(), unit()),
    );
    vec![
        (
            "written closed cell rejects logging function",
            service_order_program(
                log_stage(),
                Term::Let(
                    "cell".into(),
                    Some(Rc::new(closed_cell.clone())),
                    Rc::new(Term::app(Term::konst(Const::RefMake), v("quiet"))),
                    Rc::new(Term::apps(
                        Term::konst(Const::RefSet),
                        [v("cell"), logging()],
                    )),
                ),
            ),
        ),
        (
            "written closed parameter rejects logging function",
            service_order_program(
                log_stage(),
                Term::Let(
                    "run_q".into(),
                    Some(Rc::new(closed_param)),
                    Rc::new(Term::lam("f", Term::app(v("f"), Term::unit()))),
                    Rc::new(Term::app(v("run_q"), logging())),
                ),
            ),
        ),
        (
            "written closed branch annotation rejects logging branch",
            service_order_program(
                log_stage(),
                Term::Let(
                    "f".into(),
                    Some(Rc::new(quiet_scheme)),
                    Rc::new(bool_branch(v("quiet"), logging())),
                    Rc::new(Term::app(v("f"), Term::unit())),
                ),
            ),
        ),
        (
            "unannotated cell updated with Log still rejected when called without Log",
            service_order_program(
                none(),
                Term::Let(
                    "cell".into(),
                    None,
                    Rc::new(Term::app(Term::konst(Const::RefMake), v("quiet"))),
                    Rc::new(Term::Let(
                        "_".into(),
                        None,
                        Rc::new(Term::apps(
                            Term::konst(Const::RefSet),
                            [v("cell"), logging()],
                        )),
                        Rc::new(Term::app(
                            Term::app(Term::konst(Const::RefGet), v("cell")),
                            Term::unit(),
                        )),
                    )),
                ),
            ),
        ),
        (
            "invariant Ref does not widen closed row",
            service_order_program(
                log_stage(),
                Term::Let(
                    "cell".into(),
                    Some(Rc::new(closed_cell)),
                    Rc::new(Term::app(Term::konst(Const::RefMake), v("quiet"))),
                    Rc::new(Term::Let(
                        "widened".into(),
                        Some(Rc::new(Scheme::new(
                            Vec::new(),
                            Type::Ref(Rc::new(Type::fun(unit(), log_stage(), unit()))),
                        ))),
                        Rc::new(v("cell")),
                        Rc::new(Term::unit()),
                    )),
                ),
            ),
        ),
    ]
}

/// Issue #762: a function or task with a closed row fixing a type variable
/// widens covariantly so a later function/task needing a service fits, while
/// written closed rows and invariant `Ref`s still reject.
#[test]
fn live_core_widens_closed_rows_in_either_order() {
    for (name, p) in &service_order_accepted() {
        let (live, outcome) = probe(name, p);
        assert!(
            !matches!(&outcome, Outcome::Stuck(_)),
            "{name}: {outcome:?}"
        );
        assert!(live.is_ok(), "{name}: the model rejects: {live:?}");
        let errors = core(p);
        assert!(errors.is_empty(), "{name}: flick-core rejects: {errors:?}");
    }
    for (name, p) in &service_order_rejected() {
        let live = verdict(p);
        assert!(
            live.is_err(),
            "{name}: the model unexpectedly accepted a rejected case"
        );
        let errors = core(p);
        assert!(
            !errors.is_empty(),
            "{name}: flick-core unexpectedly accepted a rejected case"
        );
    }
}

#[test]
fn service_order_fixture_checks_in_model() {
    use flick_model::run;
    use std::path::Path as FsPath;

    let path = FsPath::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/check/pass/effects/service_order.fl");
    let modules = run::compile(&path).expect("flick-core accepts service_order.fl");
    let errors = run::check(&modules, &run::program(&modules));
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}
