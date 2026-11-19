//! The decision graph against the meaning of a `match`: on every small
//! value, walking the graph takes the arm, binds the names and runs the
//! guards, in order, that trying the arms one by one does.

use super::{Graph, Node, NodeId, Place, Step, Test, build};
use crate::ir::{Arm, Constructor, Expr, ExprKind, FileId, Lit, Pat, Rep, Span};

/// A value of the scrutinee.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum V {
    Con(&'static str, Vec<V>),
    Int(i32),
    List(Vec<V>),
    Tuple(Vec<V>),
    Rec(Vec<(&'static str, V)>),
}

/// `type T = A | B T | C T T | D Int`, and `Wrap T` unboxed.
fn con(name: &str, arity: usize) -> Constructor {
    Constructor {
        module: "M".into(),
        ty: if name == "Wrap" { "W" } else { "T" }.into(),
        name: name.into(),
        arity,
        rep: if name == "Wrap" {
            Rep::Unboxed
        } else {
            Rep::Tagged
        },
        dicts: 0,
    }
}

/// What a value names its constructor by: `K` of `M`, `N.K` of another module `N`.
fn tag(c: &Constructor) -> String {
    if c.module == "M" {
        c.name.clone()
    } else {
        format!("{}.{}", c.module, c.name)
    }
}

fn c(name: &str, args: Vec<Pat>) -> Pat {
    Pat::Con(con(name, args.len()), args)
}

fn bind(x: &str) -> Pat {
    Pat::Bind(x.into())
}

fn int(n: i32) -> Pat {
    Pat::Lit(Lit::Int(n))
}

fn arm(pat: Pat, guarded: bool) -> Arm {
    let unit = || Expr::new(ExprKind::Lit(Lit::Unit), Span::new(FileId::new(0), 0, 0));
    Arm {
        pat,
        guard: guarded.then(unit),
        body: unit(),
    }
}

/// Values of `T` up to `depth` constructors deep.
fn ts(depth: usize) -> Vec<V> {
    let mut out = vec![V::Con("A", vec![]), V::Con("D", vec![V::Int(1)])];
    out.push(V::Con("D", vec![V::Int(2)]));
    if depth > 0 {
        let inner = ts(depth - 1);
        for x in &inner {
            out.push(V::Con("B", vec![x.clone()]));
            for y in inner.iter().take(4) {
                out.push(V::Con("C", vec![x.clone(), y.clone()]));
            }
        }
    }
    out
}

/// Lists of up to three of `0`, `1` and `2`.
fn lists() -> Vec<V> {
    let mut out = vec![V::List(vec![])];
    let mut last = vec![vec![]];
    for _ in 0..3 {
        let mut next = Vec::new();
        for l in &last {
            for n in 0..3 {
                let mut l: Vec<V> = l.clone();
                l.push(V::Int(n));
                out.push(V::List(l.clone()));
                next.push(l);
            }
        }
        last = next;
    }
    out
}

/// Whether `p` matches `v`, binding its names as a sequential match does.
fn matches(p: &Pat, v: &V, binds: &mut Vec<(String, V)>) -> bool {
    match (p, v) {
        (Pat::Wild | Pat::Lit(Lit::Unit), _) => true,
        (Pat::Bind(x), v) => {
            binds.push((x.clone(), v.clone()));
            true
        }
        (Pat::Lit(Lit::Int(n)), V::Int(m)) => n == m,
        (Pat::Tuple(ps), V::Tuple(vs)) => ps.iter().zip(vs).all(|(p, v)| matches(p, v, binds)),
        (Pat::Record(fields), V::Rec(vs)) => fields.iter().all(|(l, p)| {
            let v = &vs.iter().find(|(m, _)| m == l).expect("a field").1;
            matches(p, v, binds)
        }),
        (Pat::List(items, rest), V::List(vs)) => {
            let fits = match rest {
                Some(_) => vs.len() >= items.len(),
                None => vs.len() == items.len(),
            };
            fits && items.iter().zip(vs).all(|(p, v)| matches(p, v, binds))
                && rest
                    .as_deref()
                    .is_none_or(|r| matches(r, &V::List(vs[items.len()..].to_vec()), binds))
        }
        (Pat::Con(c, args), v) if c.rep == Rep::Unboxed => matches(&args[0], v, binds),
        (Pat::Con(c, args), V::Con(name, fields)) => {
            tag(c) == *name && args.iter().zip(fields).all(|(p, v)| matches(p, v, binds))
        }
        (Pat::Or(alts), v) => alts.iter().any(|alt| {
            let mut own = Vec::new();
            let hit = matches(alt, v, &mut own);
            if hit {
                binds.extend(own);
            }
            hit
        }),
        _ => false,
    }
}

/// Whether the guard of `arm` holds with `binds`: a fixed answer for each.
fn holds(arm: usize, binds: &[(String, V)]) -> bool {
    let weight: usize = binds
        .iter()
        .map(|(x, v)| x.len() + format!("{v:?}").len())
        .sum();
    !(weight + arm).is_multiple_of(3)
}

/// The arm taken and its names, and the guards run, in order.
type Outcome = (Option<(usize, Vec<(String, V)>)>, Vec<usize>);

/// The arm a sequential match takes and its names, and the guards it ran.
fn sequential(arms: &[Arm], v: &V) -> Outcome {
    let mut ran = Vec::new();
    for (i, a) in arms.iter().enumerate() {
        let mut binds = Vec::new();
        if !matches(&a.pat, v, &mut binds) {
            continue;
        }
        if a.guard.is_some() {
            ran.push(i);
            if !holds(i, &binds) {
                continue;
            }
        }
        binds.sort();
        return (Some((i, binds)), ran);
    }
    (None, ran)
}

/// The value at `place` of `v`.
fn at(graph: &Graph, place: Place, v: &V) -> V {
    let Some((from, step)) = &graph.places[place] else {
        return v.clone();
    };
    match (step, at(graph, *from, v)) {
        (Step::Field(_, i), V::Con(_, fields)) => fields[*i].clone(),
        (Step::Item(i), V::Tuple(items) | V::List(items)) => items[*i].clone(),
        (Step::Rest(n), V::List(items)) => V::List(items[*n..].to_vec()),
        (Step::Label(l), V::Rec(fields)) => fields
            .iter()
            .find(|(m, _)| m == l)
            .expect("a field")
            .1
            .clone(),
        (step, v) => panic!("{step:?} of {v:?}"),
    }
}

fn passes(test: &Test, v: &V) -> bool {
    match (test, v) {
        (Test::Tag(c), V::Con(name, _)) => tag(c) == *name,
        (Test::Lit(Lit::Int(n)), V::Int(m)) => n == m,
        (Test::Len { n, rest }, V::List(items)) => {
            if *rest {
                items.len() >= *n
            } else {
                items.len() == *n
            }
        }
        (test, v) => panic!("{test:?} of {v:?}"),
    }
}

/// What walking `graph` on `v` gives, as [`sequential`].
fn walk(graph: &Graph, v: &V) -> Outcome {
    let mut ran = Vec::new();
    let mut node = graph.root;
    loop {
        match &graph.nodes[node] {
            Node::Switch {
                place,
                cases,
                default,
            } => {
                let value = at(graph, *place, v);
                let hits: Vec<NodeId> = (cases.iter())
                    .filter(|(tests, _)| tests.iter().any(|t| passes(t, &value)))
                    .map(|(_, n)| *n)
                    .collect();
                assert!(hits.len() <= 1, "two cases hold of {value:?}");
                node = hits.first().copied().unwrap_or(*default);
            }
            Node::Guard {
                arm,
                binds,
                otherwise,
            } => {
                let mut binds: Vec<(String, V)> = (binds.iter())
                    .map(|(x, p)| ((*x).to_owned(), at(graph, *p, v)))
                    .collect();
                binds.sort();
                ran.push(*arm);
                if holds(*arm, &binds) {
                    return (Some((*arm, binds)), ran);
                }
                node = *otherwise;
            }
            Node::Leaf { arm, binds } => {
                let mut binds: Vec<(String, V)> = (binds.iter())
                    .map(|(x, p)| ((*x).to_owned(), at(graph, *p, v)))
                    .collect();
                binds.sort();
                return (Some((*arm, binds)), ran);
            }
            Node::Fail => return (None, ran),
        }
    }
}

fn agrees<'a>(arms: &'a [Arm], values: &[V]) -> Graph<'a> {
    let graph = build(arms).expect("a graph");
    for v in values {
        assert_eq!(walk(&graph, v), sequential(arms, v), "on {v:?}");
    }
    graph
}

/// The switches on the scrutinee itself.
fn switches_at(graph: &Graph, place: Place) -> usize {
    (graph.nodes.iter())
        .filter(|n| matches!(n, Node::Switch { place: p, .. } if *p == place))
        .count()
}

#[test]
fn arms_of_one_constructor_share_its_test() {
    let arms = [
        arm(c("B", vec![c("A", vec![])]), false),
        arm(c("C", vec![c("A", vec![]), bind("x")]), false),
        arm(c("B", vec![c("B", vec![bind("y")])]), false),
        arm(c("C", vec![bind("x"), c("D", vec![int(1)])]), false),
        arm(c("B", vec![bind("z")]), false),
        arm(bind("w"), false),
    ];
    let graph = agrees(&arms, &ts(2));
    // one test of the scrutinee's constructor for all five
    assert_eq!(switches_at(&graph, 0), 1, "{graph:#?}");
    let Node::Switch { cases, .. } = &graph.nodes[graph.root] else {
        panic!("{graph:#?}");
    };
    let tags: Vec<&str> = (cases.iter())
        .flat_map(|(tests, _)| tests)
        .map(|t| match t {
            Test::Tag(c) => c.name.as_str(),
            t => panic!("{t:?}"),
        })
        .collect();
    assert_eq!(tags, ["B", "C"]);
}

#[test]
fn guards_run_in_order_once_each() {
    let arms = [
        arm(c("D", vec![int(1)]), true),
        arm(c("D", vec![bind("n")]), true),
        arm(c("B", vec![bind("x")]), true),
        arm(c("D", vec![int(2)]), false),
        arm(Pat::Wild, true),
        arm(c("B", vec![c("A", vec![])]), false),
        arm(bind("other"), false),
    ];
    agrees(&arms, &ts(2));
}

#[test]
fn an_unmatched_value_fails() {
    let arms = [
        arm(c("B", vec![c("A", vec![])]), false),
        arm(c("D", vec![bind("n")]), true),
    ];
    let graph = agrees(&arms, &ts(2));
    assert!(graph.nodes.iter().any(|n| matches!(n, Node::Fail)));
}

#[test]
fn lists_by_length_and_rest() {
    let list = |items: Vec<Pat>, rest: Option<Pat>| Pat::List(items, rest.map(Box::new));
    let arms = [
        arm(list(vec![], None), false),
        arm(list(vec![bind("x")], None), true),
        arm(list(vec![int(1)], Some(bind("r"))), false),
        arm(list(vec![bind("a"), int(2)], None), false),
        arm(list(vec![bind("a"), bind("b")], Some(bind("rest"))), true),
        arm(list(vec![int(0), int(0), int(0)], None), false),
        arm(Pat::Wild, false),
    ];
    agrees(&arms, &lists());
}

#[test]
fn or_patterns_bind_the_first_alternative_and_guard_once() {
    let pair = |a: Pat, b: Pat| Pat::Tuple(vec![a, b]);
    let d = |p: Pat| c("D", vec![p]);
    let arms = [
        arm(
            Pat::Or(vec![
                pair(d(int(1)), bind("x")),
                pair(bind("x"), d(int(2))),
                pair(c("B", vec![bind("x")]), Pat::Wild),
            ]),
            true,
        ),
        arm(
            pair(
                Pat::Or(vec![d(bind("n")), c("B", vec![d(bind("n"))])]),
                bind("y"),
            ),
            false,
        ),
        arm(pair(bind("p"), Pat::Wild), true),
        arm(Pat::Wild, false),
    ];
    let values: Vec<V> = (ts(1).iter())
        .flat_map(|a| ts(1).into_iter().map(move |b| V::Tuple(vec![a.clone(), b])))
        .collect();
    agrees(&arms, &values);
}

#[test]
fn records_unboxed_and_nested_tuples() {
    let rec = |fields: Vec<(&str, Pat)>| {
        Pat::Record(fields.into_iter().map(|(l, p)| (l.to_owned(), p)).collect())
    };
    let wrap = |p: Pat| c("Wrap", vec![p]);
    let arms = [
        arm(
            rec(vec![("a", wrap(c("D", vec![int(1)]))), ("b", bind("x"))]),
            false,
        ),
        arm(
            rec(vec![("b", Pat::Tuple(vec![c("A", vec![]), bind("y")]))]),
            true,
        ),
        arm(rec(vec![("a", wrap(bind("w")))]), false),
    ];
    let values: Vec<V> = (ts(1).iter())
        .flat_map(|a| {
            ts(0)
                .into_iter()
                .map(move |b| V::Rec(vec![("a", a.clone()), ("b", V::Tuple(vec![b.clone(), b]))]))
        })
        .collect();
    agrees(&arms, &values);
}

#[test]
fn a_match_too_big_has_no_graph() {
    // each or-pattern doubles the rows: two to the twelfth leaves
    let either = || Pat::Or(vec![c("A", vec![]), c("B", vec![Pat::Wild])]);
    let arms = [
        arm(Pat::Tuple((0..12).map(|_| either()).collect()), false),
        arm(Pat::Wild, false),
    ];
    assert!(build(&arms).is_none());
}

#[test]
fn a_catch_all_after_failed_tests_is_written_once() {
    let arms = [
        arm(c("B", vec![c("A", vec![])]), false),
        arm(c("C", vec![c("A", vec![]), Pat::Wild]), false),
        arm(bind("v"), false),
    ];
    let graph = agrees(&arms, &ts(1));
    // the tests that fail under `B` and `C` fall to the default, `v`
    assert_eq!(graph.reached(arms.len()), [1, 1, 1], "{graph:#?}");
}

#[test]
fn an_arm_on_several_branches_is_reached_from_each() {
    let arms = [
        arm(
            Pat::Or(vec![
                c("B", vec![bind("x")]),
                c("C", vec![bind("x"), Pat::Wild]),
            ]),
            true,
        ),
        arm(Pat::Wild, false),
    ];
    let graph = agrees(&arms, &ts(1));
    // the guard under `B` and under `C`, one body to go to
    assert_eq!(graph.reached(arms.len()), [2, 1], "{graph:#?}");
}

#[test]
fn alternatives_that_bind_nothing_are_one_case() {
    let arms = [
        arm(c("A", vec![]), false),
        arm(
            Pat::Or(vec![c("B", vec![Pat::Wild]), c("D", vec![Pat::Wild])]),
            false,
        ),
        arm(bind("v"), false),
    ];
    let graph = agrees(&arms, &ts(1));
    let Node::Switch { cases, .. } = &graph.nodes[graph.root] else {
        panic!("{graph:#?}");
    };
    assert_eq!(cases.len(), 2, "{graph:#?}");
    assert_eq!(cases[1].0.len(), 2, "{graph:#?}");
    assert_eq!(graph.reached(arms.len()), [1, 1, 1], "{graph:#?}");
}

#[test]
fn tests_failing_to_one_arm_are_one_test() {
    let pair = |a: Pat, b: Pat| Pat::Tuple(vec![a, b]);
    let arms = [
        arm(
            pair(c("B", vec![bind("a")]), c("B", vec![bind("b")])),
            false,
        ),
        arm(Pat::Wild, false),
    ];
    let values: Vec<V> = (ts(1).iter())
        .flat_map(|a| ts(1).into_iter().map(move |b| V::Tuple(vec![a.clone(), b])))
        .collect();
    let graph = agrees(&arms, &values);
    let Node::Switch { cases, default, .. } = &graph.nodes[graph.root] else {
        panic!("{graph:#?}");
    };
    // the test of the second tag is part of the first's
    let (more, next) = graph.chain(cases[0].1, *default);
    assert_eq!(more.len(), 1, "{graph:#?}");
    assert!(
        matches!(graph.nodes[next], Node::Leaf { arm: 0, .. }),
        "{graph:#?}"
    );
    // the catch-all is written once, where the test fails
    assert_eq!(graph.reached(arms.len()), [1, 1], "{graph:#?}");
}

/// Errors of one name from two modules, which a `match` on a value of
/// `Errors ε` meets, are two tests, and their fields two places.
#[test]
fn constructors_of_one_name_in_two_modules_are_two_tests() {
    let not_found = |module: &str| Constructor {
        module: module.into(),
        ty: "NotFound".into(),
        name: "NotFound".into(),
        arity: 1,
        rep: Rep::Tagged,
        dicts: 0,
    };
    let arms = [
        arm(Pat::Con(not_found("Http"), vec![bind("path")]), false),
        arm(Pat::Con(not_found("Db"), vec![bind("id")]), false),
    ];
    let values = [
        V::Con("Http.NotFound", vec![V::Int(1)]),
        V::Con("Db.NotFound", vec![V::Int(2)]),
    ];
    let graph = agrees(&arms, &values);
    assert_eq!(graph.places.len(), 3, "{graph:#?}");
}
