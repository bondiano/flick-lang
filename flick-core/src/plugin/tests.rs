use std::sync::Arc;

use flick_plugin::{
    Arena, Builder, Compiler, Decl, Expr, Field, HoleKind, Kind, Node as PluginNode, NodeId, Pat,
    Place, Span, Ty,
};

use super::render::render;
use super::source::BlockSource;
use super::splice::splice;
use crate::source::SourceMap;
use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::parse;

fn span(start: u32, end: u32) -> Span {
    Span { start, end }
}

/// The text of the arena `build` makes, for a body of 100 bytes.
fn text(build: impl FnOnce(&mut Builder) -> NodeId) -> Result<String, String> {
    let mut b = Builder::new(span(0, 0));
    let root = build(&mut b);
    render(&b.finish(root), 100, Place::Expression, &[]).map(|r| r.text)
}

fn field(label: &str, value: NodeId) -> Field {
    Field {
        label: label.into(),
        value,
    }
}

#[test]
fn every_kind_of_node_renders_in_parentheses() {
    let name = |b: &mut Builder, n: &str| b.expr(Expr::Name(n.into()));
    assert_eq!(text(|b| name(b, "List.map")), Ok("List.map".into()));
    assert_eq!(text(|b| b.expr(Expr::Number(1.5))), Ok("1.5".into()));
    assert_eq!(text(|b| b.expr(Expr::Number(-2.0))), Ok("(-2.0)".into()));
    assert_eq!(
        text(|b| b.expr(Expr::Text("a\"b\\c\n\u{1}".into()))),
        Ok(r#""a\"b\\c\n\u{1}""#.into())
    );
    assert_eq!(text(|b| b.expr(Expr::Boolean(true))), Ok("true".into()));
    assert_eq!(text(|b| b.expr(Expr::Unit)), Ok("()".into()));
    let applied = text(|b| {
        let f = name(b, "f");
        let x = name(b, "x");
        let y = b.expr(Expr::Number(1.0));
        b.expr(Expr::Apply {
            function: f,
            arguments: vec![x, y],
        })
    });
    assert_eq!(applied, Ok("(f x 1.0)".into()));
    let lambda = text(|b| {
        let p = b.pat(Pat::Var("p".into()));
        let w = b.pat(Pat::Wildcard);
        let inner = b.pat(Pat::Var("v".into()));
        let con = b.pat(Pat::Con {
            name: "Maybe.Just".into(),
            arguments: vec![inner],
        });
        let rec_v = b.pat(Pat::Var("r".into()));
        let rec = b.pat(Pat::Record(vec![field("a", rec_v)]));
        let body = name(b, "p");
        b.expr(Expr::Lambda {
            params: vec![p, w, con, rec],
            body,
        })
    });
    assert_eq!(lambda, Ok("(fun p _ (Maybe.Just v) { a = r } -> p)".into()));
    let record = text(|b| {
        let x = name(b, "x");
        let r = b.expr(Expr::Record(vec![field("a", x)]));
        let get = b.expr(Expr::Get {
            record: r,
            label: "a".into(),
        });
        let one = b.expr(Expr::Number(1.0));
        let items = b.expr(Expr::Items(vec![get, one]));
        let two = b.expr(Expr::Number(2.0));
        b.expr(Expr::Tuple(vec![items, two]))
    });
    assert_eq!(record, Ok("([({ a = x }).a, 1.0], 2.0)".into()));
    let annot = text(|b| {
        let e = name(b, "e");
        let list = b.ty(Ty::Name("List".into()));
        let a = b.ty(Ty::Name("a".into()));
        let applied = b.ty(Ty::Apply {
            function: list,
            arguments: vec![a],
        });
        let int = b.ty(Ty::Name("Int".into()));
        let record = b.ty(Ty::Record(vec![field("n", int)]));
        let ty = b.ty(Ty::Function {
            input: applied,
            output: record,
        });
        b.expr(Expr::Annot { expr: e, ty })
    });
    assert_eq!(annot, Ok("(e : ((List a) -> { n : Int }))".into()));
    let holes = text(|b| {
        let e = b.expr(Expr::Hole(span(1, 3)));
        let t = b.ty(Ty::Hole(span(5, 8)));
        b.expr(Expr::Annot { expr: e, ty: t })
    });
    assert_eq!(holes, Ok("((hole) : (Hole))".into()));
    let label = text(|b| {
        let e = name(b, "Record.label");
        let label = b.ty(Ty::Name("Record.Label".into()));
        let loud = b.ty(Ty::Text("lo\"ud".into()));
        let wild = b.ty(Ty::Wildcard);
        let ty = b.ty(Ty::Apply {
            function: label,
            arguments: vec![loud, wild],
        });
        b.expr(Expr::Annot { expr: e, ty })
    });
    assert_eq!(
        label,
        Ok(r#"(Record.label : (Record.Label "lo\"ud" _))"#.into())
    );
}

#[test]
fn generated_code_is_checked_before_it_is_rendered() {
    let refused = |arena: Arena| render(&arena, 10, Place::Expression, &[]).expect_err("refused");
    let node = |kind: Kind| PluginNode {
        kind,
        origin: span(0, 1),
    };
    let ahead = Arena {
        nodes: vec![
            node(Kind::Expr(Expr::Items(vec![1]))),
            node(Kind::Expr(Expr::Unit)),
        ],
        root: 0,
    };
    assert!(refused(ahead).contains("after it"));
    let wrong_role = Arena {
        nodes: vec![node(Kind::Ty(Ty::Name("Int".into())))],
        root: 0,
    };
    assert!(refused(wrong_role).contains("is not a Expr"));
    let outside = Arena {
        nodes: vec![node(Kind::Expr(Expr::Hole(span(5, 20))))],
        root: 0,
    };
    assert!(refused(outside).contains("outside the body"));
    let origin = Arena {
        nodes: vec![PluginNode {
            kind: Kind::Expr(Expr::Unit),
            origin: span(0, 11),
        }],
        root: 0,
    };
    assert!(refused(origin).contains("outside the body"));
    let twice = Arena {
        nodes: vec![
            node(Kind::Expr(Expr::Hole(span(1, 3)))),
            node(Kind::Expr(Expr::Items(vec![0, 0]))),
        ],
        root: 1,
    };
    assert!(refused(twice).contains("used twice"));
    for (kind, why) in [
        (Expr::Hole(span(2, 2)), "empty hole"),
        (Expr::Name("let".into()), "is not a name"),
        (Expr::Name("a b".into()), "is not a name"),
        (Expr::Name("list.Map".into()), "is not a name"),
        (Expr::Number(f64::NAN), "no number"),
        (Expr::Record(Vec::new()), "empty record"),
        (Expr::Tuple(Vec::new()), "fewer than two"),
    ] {
        let arena = Arena {
            nodes: vec![node(Kind::Expr(kind))],
            root: 0,
        };
        assert!(refused(arena).contains(why), "{why}");
    }
    let mut deep = vec![node(Kind::Expr(Expr::Unit))];
    for i in 0..205 {
        deep.push(node(Kind::Expr(Expr::Items(vec![i]))));
    }
    let root = 205;
    assert!(refused(Arena { nodes: deep, root }).contains("deeper"));
}

const SRC: &str = "module App.Main exposing (x)\n\nimport Pairs exposing (plugin pairs)\n\nlet x =\n  pairs { a = f 1; b : Int = 2 }\n";

fn find(node: &Node, kind: K) -> Option<&Node> {
    if node.kind == kind {
        return Some(node);
    }
    node.nodes().find_map(|n| find(n, kind))
}

fn tokens(node: &Node, out: &mut Vec<(K, u32, u32)>) {
    for child in &node.children {
        match child {
            Element::Node(n) => tokens(n, out),
            Element::Token(t) if !t.kind.is_trivia() => out.push((t.kind, t.start, t.end)),
            Element::Token(_) => {}
        }
    }
}

#[test]
fn a_block_shows_its_plugin_the_body_and_parses_holes() {
    let mut map = SourceMap::default();
    let file = map.add("Main.fl", SRC);
    let parsed = parse(SRC, file);
    let block = find(&parsed.tree, K::PluginExpr).expect("a block");
    let mut source = BlockSource::new(Arc::from(SRC), file, block, Arc::from([])).expect("closed");
    let shown = source.block();
    assert_eq!(shown.head, "pairs");
    assert_eq!(shown.text, " a = f 1; b : Int = 2 ");
    let words: Vec<&str> = shown
        .tokens
        .iter()
        .map(|t| &shown.text[t.span.start as usize..t.span.end as usize])
        .collect();
    assert_eq!(words, ["a", "=", "f", "1", ";", "b", ":", "Int", "=", "2"]);
    let len = u32::try_from(shown.text.len()).expect("short");
    let at = |word: &str| u32::try_from(shown.text.find(word).expect("there")).expect("short");
    let value = source.parse_expr(at("f 1"), len).expect("an expression");
    assert_eq!(value.kind, HoleKind::Expr);
    assert_eq!(value.span, span(at("f 1"), at("f 1") + 3));
    let ty = source.parse_type(at("Int"), len).expect("a type");
    assert_eq!(ty.span, span(at("Int"), at("Int") + 3));
    // a hole that does not parse is refused, its error kept
    assert!(source.parse_expr(at(";"), len).is_err());
}

#[test]
fn a_splice_keeps_the_block_range_and_the_holes_ranges() {
    let mut map = SourceMap::default();
    let file = map.add("Main.fl", SRC);
    let parsed = parse(SRC, file);
    let block = find(&parsed.tree, K::PluginExpr).expect("a block");
    let mut source = BlockSource::new(Arc::from(SRC), file, block, Arc::from([])).expect("closed");
    let body = source.block().text;
    let len = u32::try_from(body.len()).expect("short");
    let at = |word: &str| u32::try_from(body.find(word).expect("there")).expect("short");
    let value = source.parse_expr(at("f 1"), len).expect("an expression");
    let int = source.parse_type(at("Int"), len).expect("a type");
    let two = source.parse_expr(at("2"), len).expect("an expression");
    let mut b = Builder::new(span(0, len));
    let a = b.at(value.span).expr(Expr::Hole(value.span));
    let two = b.at(two.span).expr(Expr::Hole(two.span));
    let int = b.at(int.span).ty(Ty::Hole(int.span));
    let annot = b.expr(Expr::Annot { expr: two, ty: int });
    let record = b
        .at(span(0, len))
        .expr(Expr::Record(vec![field("a", a), field("b", annot)]));
    let rendered = render(&b.finish(record), len, Place::Expression, &[]).expect("renders");
    let expanded = format!("{SRC}\n{}", rendered.text);
    let place = u32::try_from(SRC.len() + 1).expect("short");
    let tree = splice(&expanded, file, place, &rendered, &source, block).expect("splices");
    assert_eq!(tree.range(), block.range());
    let (open, close) = (source.start(), source.start() + len);
    let mut seen = Vec::new();
    tokens(&tree, &mut seen);
    for &(kind, start, end) in &seen {
        let user = start >= open && end <= close;
        let generated = start >= place;
        let ends = (start, end) == (block.range().0, block.range().0 + 5) || end == block.range().1;
        assert!(user || generated || ends, "{kind:?} at {start}..{end}");
    }
    let users: Vec<&str> = seen
        .iter()
        .filter(|&&(_, start, _)| start >= open && start < close)
        .map(|&(_, start, end)| &SRC[start as usize..end as usize])
        .collect();
    assert_eq!(users, ["f", "1", "2", "Int"]);
}

#[test]
fn renders_a_recursive_let_and_a_forall() {
    let mut b = Builder::new(span(0, 10));
    let item = b.ty(Ty::Name("x".into()));
    let schema = b.ty(Ty::Name("Schema".into()));
    let applied = b.ty(Ty::Apply {
        function: schema,
        arguments: vec![item],
    });
    let forall = b.ty(Ty::Forall {
        vars: vec!["x".into()],
        body: applied,
    });
    let body = b.expr(Expr::Name("go".into()));
    let made = b.decl(Decl::Let {
        name: "go".into(),
        params: Vec::new(),
        ty: Some(forall),
        body,
        rec: true,
    });
    let rendered = render(&b.finish(made), 10, Place::Declaration, &[]).expect("renders");
    assert_eq!(rendered.text, "let rec go : (forall x. (Schema x)) = go");
}
