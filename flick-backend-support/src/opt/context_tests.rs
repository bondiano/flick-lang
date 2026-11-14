use crate::{Known, opt};
use flick_ir::{Dict, Expr, ExprKind, FileId, LangItem, Lit, Namespace, Span, Stage};

fn table() -> Vec<LangItem> {
    [("numeric", "Closed"), ("int", "Scalar"), ("add", "plus")]
        .into_iter()
        .map(|(lang, name)| LangItem {
            lang: lang.into(),
            module: "Renamed".into(),
            namespace: if lang == "add" {
                Namespace::Value
            } else {
                Namespace::Type
            },
            name: name.into(),
        })
        .collect()
}

fn call(module: &str, method: &str) -> Expr {
    let at = Span::new(FileId::new(0), 0, 0);
    let method = Expr::new(
        ExprKind::Method {
            dict: Dict::Instance {
                module: module.into(),
                name: "Closed Scalar".into(),
                args: vec![],
            },
            name: method.into(),
        },
        at,
    );
    let apply = |func, value| {
        Expr::new(
            ExprKind::App {
                func: Box::new(func),
                arg: Box::new(Expr::new(ExprKind::Lit(Lit::Int(value)), at)),
                stage: Stage::Pure,
                arg_first: false,
            },
            at,
        )
    };
    apply(apply(method, 20), 22)
}

#[test]
fn specialization_uses_declared_roles_instead_of_prelude_names() {
    let table = table();
    let known = Known::of(&table);
    let mut value = call("Renamed", "plus");
    opt::simplify(&mut value, &known);
    assert_eq!(value.kind, ExprKind::Lit(Lit::Int(42)));
    let mut impostor = call("Numeric", "add");
    let original = impostor.clone();
    opt::simplify(&mut impostor, &known);
    assert_eq!(impostor, original);
}

#[test]
fn missing_roles_do_not_guess_the_numeric_dictionary() {
    let known = Known::of(&[]);
    let mut value = call("Numeric", "add");
    let original = value.clone();
    opt::simplify(&mut value, &known);
    assert_eq!(value, original);
}
