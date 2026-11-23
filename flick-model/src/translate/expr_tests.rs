use super::Tr;
use crate::run::compile_source;
use crate::syntax::Term;
use flick_core::ir::{Expr, ExprKind, ExprLang, ItemKind, Lit, Pat, Stage, Stmt};
use flick_core::source::{FileId, Span};
use flick_core::types::lang::Lang;

fn expression(kind: ExprKind) -> Expr {
    Expr::new(kind, Span::new(FileId::new(0), 0, 0))
}

fn call(argument: Expr) -> Expr {
    expression(ExprKind::App {
        func: Box::new(expression(ExprKind::Global {
            module: "Test".to_owned(),
            name: "consume".to_owned(),
            type_args: Vec::new(),
        })),
        arg: Box::new(argument),
        stage: Stage::Runtime,
        arg_first: true,
    })
}

#[test]
fn argument_first_staging_keeps_task_expectations_but_not_computations() {
    let task = expression(ExprKind::Task(Box::new(expression(ExprKind::Lit(
        Lit::Unit,
    )))));
    let translated = Tr::new("Test").expr(&call(task));
    assert!(matches!(translated, Term::App(_, argument) if matches!(&*argument, Term::Task(_))));
    let computation = call(expression(ExprKind::Lit(Lit::Unit)));
    let translated = Tr::new("Test").expr(&call(computation));
    assert!(
        matches!(translated, Term::Let(_, None, value, _) if !matches!(&*value, Term::Task(_) | Term::Lam(..)))
    );
}

#[test]
fn selected_eff_worker_translates_to_the_core_await() {
    let ir = expression(ExprKind::App {
        func: Box::new(expression(ExprKind::lang(Lang::AwaitEff))),
        arg: Box::new(expression(ExprKind::Local("task".to_owned()))),
        stage: Stage::Runtime,
        arg_first: false,
    });
    let translated = Tr::new("Test").expr(&ir);
    assert!(
        matches!(translated, Term::Await(operand) if matches!(&*operand, Term::Var(name) if &**name == "task"))
    );
}

#[test]
fn selected_eff_worker_keeps_task_evaluation_outside_owner_evidence() {
    let worker = "worker".to_owned();
    let task = "task".to_owned();
    let await_call = expression(ExprKind::App {
        func: Box::new(expression(ExprKind::Local(worker.clone()))),
        arg: Box::new(expression(ExprKind::Local(task.clone()))),
        stage: Stage::Runtime,
        arg_first: false,
    });
    let bracket = expression(ExprKind::WithOwnerEvidence(
        Box::new(expression(ExprKind::Local("owner".to_owned()))),
        Box::new(await_call),
    ));
    let ir = expression(ExprKind::Block(
        vec![
            Stmt::let_bind(
                Pat::Bind(worker),
                expression(ExprKind::lang(Lang::AwaitEff)),
            ),
            Stmt::let_bind(
                Pat::Bind(task),
                call(expression(ExprKind::Local("dependency".to_owned()))),
            ),
        ],
        Box::new(bracket),
    ));
    let translated = Tr::new("Test").expr(&ir);
    let Term::Let(name, None, value, body) = translated else {
        panic!("task operand staging: {translated:?}");
    };
    assert_eq!(&*name, "task");
    assert!(matches!(&*value, Term::Let(..)));
    let Term::WithOwnerEvidence(_, inside) = &*body else {
        panic!("owner evidence: {body:?}");
    };
    assert!(
        matches!(&**inside, Term::Await(task) if matches!(&**task, Term::Var(local) if &**local == "task"))
    );
}

#[test]
fn let_else_lowers_and_translates_identically_to_canonical_match() {
    use crate::run;
    use crate::syntax::Path as ModelPath;

    let let_else_src = "module App.Main exposing (f)\n\nf : Option Int -> Int\nlet f m =\n  let Some n = m else 0\n  n + 1\n";
    let match_src = "module App.Main exposing (f)\n\nf : Option Int -> Int\nlet f m =\n  match m with\n  | Some n -> n + 1\n  | _ -> 0\n";

    let translate_f = |src: &str| {
        let modules = run::compile_source(src).expect("compiles");
        let program = run::program(&modules);
        let errors = run::check(&modules, &program);
        assert!(errors.is_empty(), "model check errors: {errors:?}");
        program.globals[&ModelPath::new("App.Main", "f")].clone()
    };

    assert_eq!(
        format!("{:?}", translate_f(let_else_src)),
        format!("{:?}", translate_f(match_src))
    );
}

#[test]
fn orelse_lowers_to_canonical_option_match() {
    let source = "module App.Main exposing (pick)\n\npick : Option Int -> Int -> Int\nlet pick opt fallback = opt orelse fallback\n";
    let modules = compile_source(source).expect("source checks");
    let main = modules
        .iter()
        .find(|module| module.ir.name == "App.Main")
        .expect("App.Main module");
    let body = main
        .ir
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Value { name, body, .. } if name == "pick" => Some(body),
            _ => None,
        })
        .expect("pick item");
    let translated = Tr::new("App.Main").expr(body);
    let mut inner = &translated;
    while let Term::Lam(_, body) = inner {
        inner = body;
    }
    assert!(
        matches!(inner, Term::Match(_, arms) if arms.len() == 2),
        "expected canonical two-arm match, got {inner:?}"
    );
}
