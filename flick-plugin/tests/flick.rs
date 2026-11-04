//! `flick!` builds the nodes its Flick says, the same a builder would.

use flick_plugin::{Builder, Decl, Expr, Field, Kind, Name, Pat, Span, Ty, Wildcard, flick};

fn span(start: u32, end: u32) -> Span {
    Span { start, end }
}

/// The kinds of the nodes `build` makes, in order.
fn kinds(build: impl FnOnce(&mut Builder)) -> Vec<Kind> {
    let mut b = Builder::new(span(0, 9));
    build(&mut b);
    let root = 0;
    b.finish(root).nodes.into_iter().map(|n| n.kind).collect()
}

#[test]
fn names_texts_holes_and_annotations() {
    let field = String::from("loud");
    let ty = span(3, 7);
    let made = kinds(|b| {
        flick!(b, Cli.switch (Record.label : Record.Label #field) (x : Cli.Multi _ #ty) "help" 1.5);
    });
    let by_hand = kinds(|b| {
        let f = b.expr(Expr::Name("Cli.switch".into()));
        let label = b.expr(Expr::Name("Record.label".into()));
        let labels = b.ty(Ty::Name("Record.Label".into()));
        let text = b.ty(Ty::Text("loud".into()));
        let labelled = b.ty(Ty::Apply {
            function: labels,
            arguments: vec![text],
        });
        let first = b.expr(Expr::Annot {
            expr: label,
            ty: labelled,
        });
        let x = b.expr(Expr::Name("x".into()));
        let multi = b.ty(Ty::Name("Cli.Multi".into()));
        let wild = b.ty(Ty::Wildcard);
        let hole = b.ty(Ty::Hole(span(3, 7)));
        let multi = b.ty(Ty::Apply {
            function: multi,
            arguments: vec![wild, hole],
        });
        let second = b.expr(Expr::Annot { expr: x, ty: multi });
        let help = b.expr(Expr::Text("help".into()));
        let n = b.expr(Expr::Number(1.5));
        b.expr(Expr::Apply {
            function: f,
            arguments: vec![first, second, help, n],
        });
    });
    assert_eq!(made, by_hand);
}

#[test]
fn spreads_records_lambdas_and_fields() {
    let holes = [span(1, 2), span(3, 4)];
    let fields = [("a", Wildcard), ("b", Wildcard)];
    let made = kinds(|b| {
        let built = flick!(b, ());
        flick!(b, f [#(..&holes), "x"] { a = #built; b = (fun { a; b = _ } -> a.c) } (1, #(..&holes)));
        flick!(b, type (#(Name("M.T")) -> { #(..fields) }));
    });
    let tail = &made[made.len() - 5..];
    assert_eq!(
        tail[..3],
        [
            Kind::Ty(Ty::Name("M.T".into())),
            Kind::Ty(Ty::Wildcard),
            Kind::Ty(Ty::Wildcard)
        ]
    );
    assert_eq!(
        tail[3],
        Kind::Ty(Ty::Record(vec![
            Field {
                label: "a".into(),
                value: u32::try_from(made.len() - 4).expect("small")
            },
            Field {
                label: "b".into(),
                value: u32::try_from(made.len() - 3).expect("small")
            },
        ]))
    );
    assert!(made.contains(&Kind::Pat(Pat::Var("a".into()))));
    assert!(made.contains(&Kind::Pat(Pat::Wildcard)));
    assert!(
        made.iter()
            .any(|k| matches!(k, Kind::Expr(Expr::Lambda { params, .. }) if params.len() == 1))
    );
    assert!(
        made.iter()
            .any(|k| matches!(k, Kind::Expr(Expr::Get { label, .. }) if label == "c"))
    );
    assert!(
        made.iter()
            .any(|k| matches!(k, Kind::Expr(Expr::Items(items)) if items.len() == 3))
    );
    assert!(
        made.iter()
            .any(|k| matches!(k, Kind::Expr(Expr::Tuple(items)) if items.len() == 3))
    );
}

#[test]
fn an_origin_is_set_for_the_nodes() {
    let mut b = Builder::new(span(0, 9));
    let word = span(2, 5);
    let root = flick!(b @ &word, Cli.group x);
    let arena = b.finish(root);
    assert!(arena.nodes.iter().all(|n| n.origin == word));
}

#[test]
fn declarations_are_built_as_written() {
    let body = span(4, 6);
    let made = kinds(|b| {
        flick!(b, let #("double") x (#body) : Float = f x);
        flick!(b, twice : Float -> Float);
        flick!(b, type alias Point a = { x : a });
        flick!(b, type Color = Red | Rgb Float Float deriving Show, Cli.Value);
        flick!(b, instance Show a => Show (Box a) { let show _ = "box"; size : Int });
    });
    let decls: Vec<&Decl> = (made.iter())
        .filter_map(|k| match k {
            Kind::Decl(d) => Some(d),
            _ => None,
        })
        .collect();
    assert!(
        matches!(decls[0], Decl::Let { name, params, ty: Some(_), .. } if name == "double" && params.len() == 2)
    );
    assert!(made.contains(&Kind::Pat(Pat::Hole(body))));
    assert!(matches!(decls[1], Decl::Signature { name, .. } if name == "twice"));
    assert!(
        matches!(decls[2], Decl::Alias { name, params, .. } if name == "Point" && params == &["a"])
    );
    let Decl::Adt {
        cases, deriving, ..
    } = decls[3]
    else {
        panic!("{:?}", decls[3]);
    };
    assert_eq!(
        cases
            .iter()
            .map(|c| (c.name.as_str(), c.fields.len()))
            .collect::<Vec<_>>(),
        [("Red", 0), ("Rgb", 2)]
    );
    assert_eq!(deriving, &["Show", "Cli.Value"]);
    // the members come before their instance
    assert!(matches!(decls[4], Decl::Let { name, .. } if name == "show"));
    assert!(matches!(decls[5], Decl::Signature { name, .. } if name == "size"));
    assert!(
        matches!(decls[6], Decl::Instance { context, members, .. } if context.len() == 1 && members.len() == 2)
    );
}

#[test]
fn type_starts_a_type_without_an_equals_and_ty_is_a_name() {
    let made = kinds(|b| {
        flick!(b, ty);
        flick!(b, type { a : Int });
        flick!(b, type Color = Red);
        flick!(b, (x, y,));
    });
    assert_eq!(made[0], Kind::Expr(Expr::Name("ty".into())));
    assert!(matches!(made[2], Kind::Ty(Ty::Record(_))), "{made:?}");
    assert!(matches!(made[3], Kind::Decl(Decl::Adt { .. })), "{made:?}");
    assert!(
        matches!(&made[6], Kind::Expr(Expr::Tuple(items)) if items.len() == 2),
        "{made:?}"
    );
}

#[test]
fn let_rec_builds_a_recursive_let() {
    let made = kinds(|b| {
        flick!(b, let rec go x = go x);
    });
    assert!(
        matches!(made.last(), Some(Kind::Decl(Decl::Let { rec: true, .. }))),
        "{made:?}"
    );
}
