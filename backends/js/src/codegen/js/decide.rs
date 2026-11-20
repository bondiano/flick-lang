//! The arms of a `match` through its decision graph
//! ([`flick_backend_support::decision`]): each test of the scrutinee is made
//! once for all the arms that need it, an `if` that ends in an arm's body
//! or falls to the default after it (see `Graph::elided`), so the next
//! test is what runs when it does not hold. A match where the graph would
//! write an arm more than once is tried arm by arm instead: the go target
//! goes to such a body, which here would need a labeled block around the
//! tests, code the browser compiler's size does not have room for. In
//! release the match is exhaustive: the last test before a failure is not
//! made.

use super::doc::Js;
use super::pattern::Test as Tested;
use super::{Gen, names};
use crate::ir::{Arm, Expr, Pat};
use flick_backend_support::decision::{self, Graph, Node, NodeId, Place, Step, Test};

/// The most arms of a match written through its graph.
const ARMS: usize = 16;

/// What the emitter knows of the graph it writes.
struct Walk<'g> {
    graph: &'g Graph<'g>,
    arms: &'g [Arm],
    /// The JS of each place, once written.
    places: Vec<Option<String>>,
    /// The variable of the value and the label of the `match` block.
    result: Option<(&'g str, &'g str)>,
}

impl<'d> Gen<'_, 'd> {
    /// The arms of a `match` on `subject` as their decision graph; `false`
    /// when there is none, or it writes an arm more than once, and the
    /// arms are tried one by one. `result` is the variable of its value and
    /// the label of its block.
    pub(super) fn decide(
        &mut self,
        subject: &str,
        arms: &[Arm],
        result: Option<(&str, &str)>,
    ) -> bool {
        // A match of many arms tries them in turn: the backend runs as a
        // component with a fixed fuel budget, which a graph of the derived
        // methods of a large enum spends for no gain. So does one of a walked
        // list, whose items and rest are read past its offset (see `view`).
        if arms.len() > ARMS || self.views.contains_key(subject) {
            return false;
        }
        let Some(graph) = decision::build(arms) else {
            return false;
        };
        if graph.reached(arms.len()).iter().any(|&n| n > 1) {
            return false;
        }
        let mut w = Walk {
            graph: &graph,
            arms,
            places: vec![None; graph.places.len()],
            result,
        };
        w.places[0] = Some(subject.to_owned());
        self.node(&mut w, graph.root, None);
        true
    }

    /// The body of an arm: its value in the variable of the `match`,
    /// leaving its block, or in tail position.
    pub(super) fn arm_body(&mut self, body: &Expr, result: Option<(&str, &str)>) {
        match result {
            Some((r, label)) => {
                let v = self.expr(body);
                self.buf.push(self.assign(r, v));
                self.buf.push(self.line(&format!("break {label};")));
            }
            None => self.tail_expr(body),
        }
    }

    /// The code of node `id`; nothing where it is the node `fall`, which
    /// the code reaches when it leaves the block (see `Graph::elided`).
    fn node(&mut self, w: &mut Walk<'_>, id: NodeId, fall: Option<NodeId>) {
        if w.graph.elided(id, fall) {
            return;
        }
        let graph = w.graph;
        match &graph.nodes[id] {
            Node::Switch {
                place,
                cases,
                default,
            } => {
                // exhaustive: what reaches a failure passes the last test
                let (cases, default) = match cases.split_last() {
                    Some(((_, last), rest))
                        if self.options.release && matches!(graph.nodes[*default], Node::Fail) =>
                    {
                        (rest, *last)
                    }
                    _ => (&cases[..], *default),
                };
                for (tests, next) in cases {
                    let (more, next) = graph.chain(*next, default);
                    let mut all = vec![(*place, &tests[..])];
                    all.extend(more);
                    let among = all.len() > 1;
                    let mut conds = Vec::with_capacity(all.len());
                    for (place, tests) in all {
                        let subject = self.place(w, place);
                        conds.push(self.any(&subject, tests, among));
                    }
                    let cond = self.chain(&conds, " && ");
                    let (stmts, ()) = self.block(|g| g.node(w, next, Some(default)));
                    let stmt = self.if_then(cond, &stmts);
                    self.buf.push(stmt);
                }
                self.node(w, default, fall);
            }
            Node::Guard {
                arm,
                binds,
                otherwise,
            } => {
                self.bind_arm(w, binds);
                let guard = w.arms[*arm].guard.as_ref().expect("a guarded arm");
                let c = self.expr(guard);
                let (then, ()) = self.block(|g| g.arm_body(&w.arms[*arm].body, w.result));
                let stmt = self.if_then(c, &then);
                self.buf.push(stmt);
                self.node(w, *otherwise, fall);
            }
            Node::Leaf { arm, binds } => {
                self.bind_arm(w, binds);
                self.arm_body(&w.arms[*arm].body, w.result);
            }
            Node::Fail => {}
        }
    }

    /// The condition that one of `tests` holds of `subject`, as the test
    /// of a pattern makes it: in parentheses when `among` others.
    fn any(&mut self, subject: &str, tests: &[Test], among: bool) -> Js<'d> {
        let mut t = Tested::new();
        for test in tests {
            let pat = match *test {
                Test::Tag(c) | Test::Class(c) | Test::Enum(c) => Pat::Con(c.clone(), Vec::new()),
                Test::Lit(lit) => Pat::Lit(lit.clone()),
                Test::Len { n, rest } => {
                    Pat::List(vec![Pat::Wild; n], rest.then(|| Box::new(Pat::Wild)))
                }
            };
            let mut one = Tested::new();
            self.test(&pat, subject, &mut one);
            t.conds.push(self.chain(&one.conds, " && "));
        }
        if t.conds.len() == 1 || !among {
            return self.chain(&t.conds, " || ");
        }
        self.chain_in_parens(&t.conds, " || ")
    }

    /// Binds the names of an arm to their places.
    fn bind_arm(&mut self, w: &mut Walk<'_>, binds: &[(&str, Place)]) {
        for (name, place) in binds {
            let path = self.place(w, *place);
            let stmt = self.constant(&names::ident(name), self.leaf(&path));
            self.buf.push(stmt);
        }
    }

    /// The JS of the value at `place`.
    fn place(&mut self, w: &mut Walk<'_>, place: Place) -> String {
        if let Some(js) = &w.places[place] {
            return js.clone();
        }
        let (from, step) = w.graph.places[place].expect("a part of the scrutinee");
        let mut js = self.place(w, from);
        let (open, part, close) = match step {
            Step::Field(c, i) => (".", self.shape(c, i + 1).1.swap_remove(i), ""),
            Step::Item(i) => ("[", i.to_string(), "]"),
            Step::Rest(n) => (".slice(", n.to_string(), ")"),
            Step::Label(label) if names::is_property(label) => (".", label.to_owned(), ""),
            Step::Label(label) => ("[", names::string(label), "]"),
        };
        js.push_str(open);
        js.push_str(&part);
        js.push_str(close);
        w.places[place] = Some(js.clone());
        js
    }
}
