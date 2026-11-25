//! A `match` through its decision graph
//! ([`flick_backend_support::decision`]), as the JS target walks it.
//!
//! A match is a block the arms leave: in tail position each arm returns,
//! and elsewhere the block is a `repeat … until true` each arm leaves with
//! `break` after delivering its value. Each test is an `if` whose block
//! ends in an arm's body or falls to the default after it (see
//! `Graph::elided`), so the next test is what runs when it does not hold.
//! Lua 5.1 has no `goto`, so a match whose graph would write an arm more
//! than once, or that has none, tries its arms one by one in the same block.

use std::iter;

use flick_backend_support::decision::{self, Graph, Node, NodeId};
use flick_ir::{Arm, Expr as IrExpr, Span};

use super::super::ast::{BinOp, Expr, ExprKind, LastKind, Name, StmtKind};
use super::pattern::{Place, Sink, all};
use super::{Dest, Lowering};

/// The most arms of a match written through its graph: a graph of the
/// derived methods of a large enum costs work for no gain.
const ARMS: usize = 16;

/// What the walk of a graph knows.
struct Walk<'g, 'p> {
    graph: &'g Graph<'p>,
    arms: &'p [Arm],
    /// Each place of the graph, once computed.
    places: Vec<Option<Place>>,
    dest: &'g Dest,
    span: Span,
}

impl Lowering<'_> {
    /// A `match`, its value delivered to `dest`.
    pub(super) fn match_expr(&mut self, scrutinee: &IrExpr, arms: &[Arm], dest: &Dest, span: Span) {
        let value = self.expr(scrutinee);
        let subject = self.subject(value);
        let body = self.within(|g| {
            if !g.decide(&subject, arms, dest, span) {
                for arm in arms {
                    g.arm(&subject, arm, dest, span);
                }
            }
        });
        match dest {
            Dest::Return => self.inline(body),
            Dest::Into(_) | Dest::Drop => {
                let once = Expr::new(ExprKind::Bool(true), span);
                self.push(StmtKind::Repeat(body, once), span);
            }
        }
    }

    /// The body of an arm, its value delivered, leaving the match.
    fn arm_body(&mut self, body: &IrExpr, dest: &Dest) {
        self.to(body, dest);
        if !matches!(dest, Dest::Return) {
            self.block.end(LastKind::Break, body.span);
        }
    }

    /// One arm, tried on its own: its tests, its names, its guard.
    fn arm(&mut self, subject: &Name, arm: &Arm, dest: &Dest, span: Span) {
        let mut checks = Vec::new();
        self.checks(
            &arm.pat,
            &Place::Value(Expr::name(subject, span)),
            span,
            &mut checks,
        );
        self.emit_checks(&checks, Sink::Declare, span, &mut |g| match &arm.guard {
            Some(guard) => {
                let c = g.expr(guard);
                let body = g.within(|g| g.arm_body(&arm.body, dest));
                g.push(StmtKind::If(vec![(c, body)], None), guard.span);
            }
            None => g.arm_body(&arm.body, dest),
        });
    }

    /// The arms through their decision graph; `false` when there is none,
    /// or it writes an arm more than once.
    fn decide(&mut self, subject: &Name, arms: &[Arm], dest: &Dest, span: Span) -> bool {
        if arms.len() > ARMS {
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
            dest,
            span,
        };
        w.places[0] = Some(Place::Value(Expr::name(subject, span)));
        self.node(&mut w, graph.root, None);
        true
    }

    /// The code of node `id`; nothing where it is the node `fall` the code
    /// reaches when it leaves the block.
    fn node(&mut self, w: &mut Walk<'_, '_>, id: NodeId, fall: Option<NodeId>) {
        if w.graph.elided(id, fall) {
            return;
        }
        let (graph, span) = (w.graph, w.span);
        match &graph.nodes[id] {
            Node::Switch {
                place,
                cases,
                default,
            } => {
                for (tests, next) in cases {
                    let (more, next) = graph.chain(*next, *default);
                    let mut conds = Vec::new();
                    for (at, tests) in iter::once((*place, &tests[..])).chain(more) {
                        let at = Self::place(w, at);
                        let any: Vec<Expr> =
                            tests.iter().map(|t| self.cond(&at, *t, span)).collect();
                        conds.extend(all(BinOp::Or, any, span));
                    }
                    let cond = all(BinOp::And, conds, span)
                        .unwrap_or(Expr::new(ExprKind::Bool(true), span));
                    let body = self.within(|g| g.node(w, next, Some(*default)));
                    self.push(StmtKind::If(vec![(cond, body)], None), span);
                }
                self.node(w, *default, fall);
            }
            Node::Guard {
                arm,
                binds,
                otherwise,
            } => {
                self.bind_arm(w, binds);
                let arm = &w.arms[*arm];
                let guard = arm.guard.as_ref().expect("a guarded arm");
                let c = self.expr(guard);
                let dest = w.dest;
                let body = self.within(|g| g.arm_body(&arm.body, dest));
                self.push(StmtKind::If(vec![(c, body)], None), guard.span);
                self.node(w, *otherwise, fall);
            }
            Node::Leaf { arm, binds } => {
                self.bind_arm(w, binds);
                let dest = w.dest;
                self.arm_body(&w.arms[*arm].body, dest);
            }
            Node::Fail => {}
        }
    }

    fn bind_arm(&mut self, w: &mut Walk<'_, '_>, binds: &[(&str, decision::Place)]) {
        for (name, at) in binds {
            let place = Self::place(w, *at);
            self.bind(Sink::Declare, name, &place, w.span);
        }
    }

    /// The expression of the part `at` of the scrutinee.
    fn place(w: &mut Walk<'_, '_>, at: decision::Place) -> Place {
        if let Some(place) = &w.places[at] {
            return place.clone();
        }
        let (from, step) = w.graph.places[at].expect("a part of the scrutinee");
        let from = Self::place(w, from);
        let place = Self::step(&from, step, w.span);
        w.places[at] = Some(place.clone());
        place
    }
}
