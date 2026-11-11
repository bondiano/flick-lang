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
    assert_eq!(text(|b| b.expr(Expr::Number(-2.0))), Ok("(-2)".into()));
    assert_eq!(text(|b| b.expr(Expr::Number(1e300))), Ok("1e300".into()));
    assert_eq!(
        text(|b| b.expr(Expr::Text("a\"b\\c\n\u{1}".into()))),
        Ok(r#""a\"b\\c\n\u{1}""#.into())
    );
    assert_eq!(text(|b| b.expr(Expr::Boolean(true))), Ok("true".into()));
    assert_eq!(text(|b| b.expr(Expr::Unit)), Ok("()".into()));
    let joined = text(|b| {
        let a = b.expr(Expr::Text("a".into()));
        let x = name(b, "x");
        b.expr(Expr::Concat(vec![a, x]))
    });
    assert_eq!(joined, Ok("(\"a\" ++ x)".into()));
    let applied = text(|b| {
        let f = name(b, "f");
        let x = name(b, "x");
        let y = b.expr(Expr::Number(1.0));
        b.expr(Expr::Apply {
            function: f,
            arguments: vec![x, y],
        })
    });
    assert_eq!(applied, Ok("(f x 1)".into()));
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
    assert_eq!(record, Ok("([({ a = x }).a, 1], 2)".into()));
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
        (Expr::Concat(Vec::new()), "fewer than two"),
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

#[test]
fn renders_a_row_of_types() {
    let mut b = Builder::new(span(0, 10));
    let found = b.ty(Ty::Name("NotFound".into()));
    let conflict = b.ty(Ty::Name("Conflict".into()));
    let errors = b.ty(Ty::Row(vec![found, conflict]));
    let none = b.ty(Ty::Row(Vec::new()));
    let cases = b.ty(Ty::Name("ErrorCases".into()));
    let both = b.ty(Ty::Apply {
        function: cases,
        arguments: vec![errors, none],
    });
    let body = b.expr(Expr::Name("cases".into()));
    let made = b.decl(Decl::Let {
        name: "go".into(),
        params: Vec::new(),
        ty: Some(both),
        body,
        rec: false,
    });
    let rendered = render(&b.finish(made), 10, Place::Declaration, &[]).expect("renders");
    assert_eq!(
        rendered.text,
        "let go : (ErrorCases {NotFound, Conflict} {}) = cases"
    );
}

#[test]
fn a_type_may_stand_twice_and_an_expression_once() {
    let mut b = Builder::new(span(0, 10));
    let pair = b.ty(Ty::Name("Pair".into()));
    let user = b.ty(Ty::Hole(span(1, 4)));
    let ty = b.ty(Ty::Apply {
        function: pair,
        arguments: vec![user, user],
    });
    let made = b.decl(Decl::Signature {
        name: "p".into(),
        ty,
    });
    let rendered = render(&b.finish(made), 10, Place::Declaration, &[]).expect("renders");
    assert_eq!(rendered.text, "p : (Pair (Hole) (Hole))");
    assert_eq!(rendered.holes.len(), 2);
}

/// A plugin linked into the compiler that panics on every block.
struct Panicking;

impl flick_plugin::Plugin for Panicking {
    fn collect(&self, _: &[flick_plugin::Block], _: &mut dyn Compiler) -> Vec<u8> {
        Vec::new()
    }

    fn expand(
        &self,
        _: &flick_plugin::Block,
        _: &[u8],
        _: &mut dyn Compiler,
    ) -> Result<Arena, Vec<flick_plugin::Diagnostic>> {
        panic!("out of bounds")
    }

    fn highlight(&self, _: &flick_plugin::Block, _: &[u8]) -> Vec<flick_plugin::Highlight> {
        panic!("no colors")
    }

    fn check_calls(&self, _: &[flick_plugin::CallSite], _: &[u8]) -> Vec<flick_plugin::Diagnostic> {
        panic!("no calls")
    }
}

#[test]
fn a_plugin_sees_the_calls_of_the_names_it_watches_and_no_others() {
    use super::Native;
    use super::calls::check_calls;
    use crate::diagnostics::codes;
    use crate::source::FileId;
    use crate::types::Watched;
    let site = flick_plugin::CallSite {
        module: "Ui.Event".into(),
        name: "queue".into(),
        span: span(10, 15),
        arguments: Vec::new(),
        refinement: None,
        binding: None,
        generated: false,
        tagged: None,
        lacking: None,
    };
    let plugin = Native::new("panicking", Panicking);
    let file = FileId::default();
    // a name its package does not watch: the plugin is not called at all
    let elsewhere = Watched::new(["Action.queue"]);
    let sites = [site.clone()];
    assert!(check_calls(&plugin, "panicking", &elsewhere, &sites, &[], file).is_empty());
    // one it watches: the plugin fails, and that is the diagnostic
    let watched = Watched::new(["Ui.Event.queue"]);
    let found = check_calls(&plugin, "panicking", &watched, &sites, &[], file);
    let [failure] = &found[..] else {
        panic!("{found:?}")
    };
    assert_eq!(failure.code, codes::P036);
    assert_eq!(failure.span.map(|s| (s.start, s.end)), Some((10, 15)));
    assert!(
        failure.message.contains("panicked: no calls"),
        "{}",
        failure.message
    );
}

#[test]
fn a_native_plugin_that_panics_fails_the_block_not_the_compiler() {
    use super::{Expander, Failure, Native};
    let mut map = SourceMap::default();
    let file = map.add("Main.fl", SRC);
    let parsed = parse(SRC, file);
    let node = find(&parsed.tree, K::PluginExpr).expect("a block");
    let mut source = BlockSource::new(Arc::from(SRC), file, node, Arc::from([])).expect("closed");
    let block = source.block();
    let plugin = Native::new("panicking", Panicking);
    match plugin.expand(&block, &[], &mut source) {
        Err(Failure::Broken(why)) => assert_eq!(why, "panicked: out of bounds"),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        plugin.highlight(&block, &[]).expect_err("it panicked"),
        "panicked: no colors"
    );
}

#[test]
fn a_match_an_if_and_a_block_render_as_the_parser_reads_them() {
    let built = text(|b| {
        let scrutinee = b.expr(Expr::Name("n".into()));
        let (yes, no) = (b.expr(Expr::Boolean(true)), b.expr(Expr::Boolean(false)));
        let condition = b.expr(Expr::Boolean(true));
        let guard = b.expr(Expr::If {
            condition,
            then: yes,
            otherwise: no,
        });
        let x = b.pat(Pat::Var("x".into()));
        let found = b.expr(Expr::Name("x".into()));
        let arms = vec![flick_plugin::Arm {
            pattern: x,
            guard: Some(guard),
            body: found,
        }];
        let chosen = b.expr(Expr::Match { scrutinee, arms });
        let one = b.expr(Expr::Number(1.0));
        let kept = b.decl(Decl::Let {
            name: "n".into(),
            params: Vec::new(),
            ty: None,
            body: one,
            rec: false,
        });
        let body = b.expr(Expr::Block {
            statements: vec![kept],
            result: chosen,
        });
        let param = b.pat(Pat::Wildcard);
        b.expr(Expr::Lambda {
            params: vec![param],
            body,
        })
    });
    assert_eq!(
        built.as_deref(),
        Ok(
            "(fun _ -> \n  let n = 1\n  (match n with | x when (if true then true else false) -> x))"
        )
    );
    // a block is the body of a `fun` or a `let`, and a `match` has arms
    let loose = text(|b| {
        let one = b.expr(Expr::Number(1.0));
        b.expr(Expr::Block {
            statements: Vec::new(),
            result: one,
        })
    });
    assert_eq!(
        loose,
        Err("a block is only the body of a `fun` or a `let`".into())
    );
    let armless = text(|b| {
        let scrutinee = b.expr(Expr::Unit);
        b.expr(Expr::Match {
            scrutinee,
            arms: Vec::new(),
        })
    });
    assert_eq!(armless, Err("a `match` without arms".into()));
}

#[test]
fn a_built_in_module_expands_a_head_of_std_without_an_import_and_says_what_it_cannot() {
    use super::derive::builtin_expansion;
    use crate::resolve::module_syntax;
    let expand = |text: &str| {
        let mut map = SourceMap::default();
        let file = map.add("Tool.fl", text);
        let parsed = parse(text, file);
        let syntax = module_syntax(&parsed.tree, text, file);
        let (expansion, diagnostics) = builtin_expansion(text, file, &parsed.tree, "Tool", &syntax);
        let codes: Vec<String> = diagnostics.iter().map(|d| d.code.to_string()).collect();
        (expansion.is_some(), codes)
    };
    let one = "module Tool exposing (tool)\n\ntool : Cli.Command none none\nlet tool = cli \"tool\" { handle fun {} -> Platform.stdout_line \"hi\" }\n";
    assert_eq!(expand(one), (true, vec![]));
    // a block in the code of another expands in a later round, up to ROUNDS deep
    let nested = |depth: usize| {
        let mut command =
            "cli \"leaf\" { handle fun {} -> Platform.stdout_line \"hi\" }".to_owned();
        for i in 0..depth {
            command = format!("cli \"c{i}\" {{ subcommand {command} }}");
        }
        format!(
            "module Tool exposing (tool)\n\ntool : Cli.Command none none\nlet tool = {command}\n"
        )
    };
    assert_eq!(expand(&nested(1)), (true, vec![]));
    assert_eq!(
        expand(&nested(super::ROUNDS + 1)),
        (true, vec!["FLICK-P036".to_owned()])
    );
}

#[test]
fn standalone_component_checks_report_source_complexity() {
    use crate::diagnostics::codes;
    use crate::source::SourceMap;
    let source = format!(
        "module App exposing (..)\ncomponent badge (p : Bool) = {}Ui.text \"нет\"\n",
        "if p then Ui.text \"да\" else ".repeat(11)
    );
    let mut sources = SourceMap::default();
    let file = sources.add("App.fl", source.clone());
    let diagnostics = crate::check(&sources, file);
    let warnings: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == codes::T038)
        .collect();
    assert_eq!(warnings.len(), 1, "{diagnostics:?}");
    assert_eq!(
        &source[warnings[0].span.expect("name span").range()],
        "badge"
    );
}
