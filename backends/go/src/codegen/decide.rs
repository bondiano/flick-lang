//! The arms of a `match` through its decision graph
//! ([`flick_backend_support::decision`]): each test of the scrutinee is made
//! once for all the arms that need it, an `if` that ends in an arm's body,
//! a jump or a failure, so the next test is what runs when it does not hold.
//! An arm the graph reaches from more than one branch has its body written
//! once after the tests: each branch assigns its names, declared before the
//! tests, and goes there with `goto`. Every body ends in a `return`, or a
//! `break` out of the loop of a `match` with a result, and the decision
//! code is a block of its own, so no `goto` passes a declaration.

use std::collections::HashMap;

use flick_pretty::{Document, Documentable, docvec};

use super::arith::Native;
use super::expr::enum_literal;
use super::layout::{block, call, statement};
use super::{Gen, names, own};
use flick_backend_support::decision::{self, Graph, Node, NodeId, Place, Step, Test};
use flick_ir::JsClass;
use flick_ir::{Arm, Expr, Lit};

/// What the emitter knows of the graph it writes.
struct Walk<'g, 'a> {
    graph: &'g Graph<'g>,
    arms: &'g [Arm],
    /// The Go of each place, once written.
    places: Vec<Option<Document<'a>>>,
    /// The label of the body of each arm reached from several branches.
    labels: Vec<Option<&'a str>>,
    /// The result variable and its native Go type, if any.
    result: Option<(&'a str, Option<Native>)>,
}

impl<'a> Gen<'a> {
    /// The arms of a `match` on `subject` as their decision graph; `false`
    /// when it has none, and the arms are tried one by one.
    pub(super) fn decide(
        &mut self,
        subject: Document<'a>,
        arms: &[Arm],
        result: Option<(&'a str, Option<Native>)>,
    ) -> bool {
        let Some(graph) = decision::build(arms) else {
            return false;
        };
        let a = self.a;
        let reached = graph.reached(arms.len());
        let labels: Vec<Option<&'a str>> = (reached.iter())
            .map(|&n| (n > 1).then(|| a.alloc_str(&format!("arm_{}", self.temp()))))
            .collect();
        let mut w = Walk {
            graph: &graph,
            arms,
            places: vec![None; graph.places.len()],
            labels,
            result,
        };
        w.places[0] = Some(subject);
        let shared = w.labels.iter().any(Option::is_some);
        let mut out = Vec::new();
        let saved_natives = self.natives.clone();
        if shared {
            // the names of an arm gone to, assigned by each branch that goes there;
            // keep a native Go type when every branch reaching the arm agrees on it
            let mut order: Vec<&str> = Vec::new();
            let mut shared_natives: HashMap<&str, Option<Native>> = HashMap::new();
            for node in &graph.nodes {
                if let Node::Guard { arm, binds, .. } | Node::Leaf { arm, binds } = node
                    && w.labels[*arm].is_some()
                {
                    for &(name, place) in binds {
                        let native = self.place_native(&w, place);
                        shared_natives
                            .entry(name)
                            .and_modify(|slot| {
                                if *slot != native {
                                    *slot = None;
                                }
                            })
                            .or_insert_with(|| {
                                order.push(name);
                                native
                            });
                    }
                }
            }
            for name in order {
                let local = a.alloc_str(&names::local(name));
                let native = shared_natives[name];
                let go_ty = native.map_or("flickrt.Value", Native::go);
                out.push(statement(a, docvec![a, "var ", local, " ", go_ty]));
                out.push(statement(a, docvec![a, "_ = ", local]));
                match native {
                    Some(n) => {
                        self.natives.insert(name.to_owned(), n);
                    }
                    None => {
                        self.natives.remove(name);
                    }
                }
            }
            let (tests, ()) = self.block(|g| g.node(&mut w, graph.root, None));
            out.push(statement(a, block(a, tests)));
            for (arm, label) in arms.iter().zip(&w.labels) {
                if let Some(label) = label {
                    let (body, ()) = self.block(|g| g.arm_body(&arm.body, result));
                    out.push(statement(a, docvec![a, *label, ": ", block(a, body)]));
                }
            }
        } else {
            let (tests, ()) = self.block(|g| g.node(&mut w, graph.root, None));
            out.extend(tests);
        }
        self.natives = saved_natives;
        if result.is_some() {
            self.emit(docvec![a, "for ", block(a, out)]);
        } else {
            self.buf.extend(out);
            if shared || !ends_in_failure(&graph) {
                self.fail(None);
            }
        }
        true
    }

    /// The body of an arm: its value in `result`, leaving the loop of the
    /// `match`, or returned.
    pub(super) fn arm_body(&mut self, body: &Expr, result: Option<(&'a str, Option<Native>)>) {
        match result {
            Some((r, want)) => {
                let v = self.expr(body);
                let v = match want {
                    Some(n) => self.unboxed(body, v, n),
                    None => v,
                };
                self.emit(docvec![self.a, r, " = ", v]);
                self.emit("break");
            }
            None => self.tail_expr(body),
        }
    }

    /// The statement of a value no arm matches.
    fn fail(&mut self, result: Option<(&'a str, Option<Native>)>) {
        self.emit(match (result, self.returns) {
            (Some(_), _) => "flickrt.NoMatch()",
            (None, Some(_)) => "panic(flickrt.NoMatch())",
            (None, None) => "return flickrt.NoMatch()",
        });
    }

    /// The code of node `id`; nothing where it is the node `fall`, which
    /// the code reaches when it leaves the block (see [`Graph::elided`]).
    fn node(&mut self, w: &mut Walk<'_, 'a>, id: NodeId, fall: Option<NodeId>) {
        if w.graph.elided(id, fall) {
            return;
        }
        let a = self.a;
        let graph = w.graph;
        match &graph.nodes[id] {
            Node::Switch {
                place,
                cases,
                default,
            } => {
                for (tests, next) in cases {
                    let (more, next) = graph.chain(*next, *default);
                    let mut conds = vec![self.any(w, *place, tests)];
                    for (place, tests) in more {
                        conds.push(self.any(w, place, tests));
                    }
                    let cond = a.join(conds, a.text(" && "));
                    let (stmts, ()) = self.block(|g| g.node(w, next, Some(*default)));
                    self.emit(docvec![a, "if ", cond, " ", block(a, stmts)]);
                }
                self.node(w, *default, fall);
            }
            Node::Guard {
                arm,
                binds,
                otherwise,
            } => {
                let added = self.bind_arm(w, *arm, binds);
                let guard = w.arms[*arm].guard.as_ref().expect("a guarded arm");
                let c = self.expr(guard);
                let test = self.unboxed(guard, c, Native::Bool).doc;
                let (then, ()) = self.block(|g| g.enter(w, *arm));
                self.emit(docvec![a, "if ", test, " ", block(a, then)]);
                for name in added {
                    self.natives.remove(&name);
                }
                self.node(w, *otherwise, fall);
            }
            Node::Leaf { arm, binds } => {
                let added = self.bind_arm(w, *arm, binds);
                self.enter(w, *arm);
                for name in added {
                    self.natives.remove(&name);
                }
            }
            Node::Fail => self.fail(w.result),
        }
    }

    /// The condition that one of `tests` holds of the value at `place`.
    fn any(&mut self, w: &mut Walk<'_, 'a>, place: Place, tests: &[Test]) -> Document<'a> {
        let a = self.a;
        let mut any = Vec::with_capacity(tests.len());
        for t in tests {
            any.push(self.condition(w, place, t));
        }
        if any.len() == 1 {
            any[0]
        } else {
            docvec![a, "(", a.join(any, a.text(" || ")), ")"]
        }
    }

    /// The body of `arm`, or a jump to it.
    fn enter(&mut self, w: &Walk<'_, 'a>, arm: usize) {
        match w.labels[arm] {
            Some(label) => self.emit(docvec![self.a, "goto ", label]),
            None => self.arm_body(&w.arms[arm].body, w.result),
        }
    }

    /// Binds the names of `arm` to their places: declared where its body
    /// is, assigned when it is gone to. Returns native locals inserted into
    /// `self.natives` for this non-shared arm so the caller can drop them on exit.
    fn bind_arm(
        &mut self,
        w: &mut Walk<'_, 'a>,
        arm: usize,
        binds: &[(&str, Place)],
    ) -> Vec<String> {
        let a = self.a;
        let mut added = Vec::new();
        for (name, place) in binds {
            let path = self.place(w, *place);
            let local = a.alloc_str(&names::local(name));
            if w.labels[arm].is_some() {
                self.emit(docvec![a, local, " = ", path]);
                continue;
            }
            if let Some(native) = self.place_native(w, *place) {
                self.bind_native(name, path.into(), native);
                added.push((*name).to_owned());
                continue;
            }
            if let Some(labels) = self.held_record(w, *place) {
                // read by place where the name is read by label
                self.held.insert(local.to_owned(), labels);
            }
            self.bind(name, path.into());
        }
        added
    }

    /// The native Go scalar type of `place`, when it reads an unboxed scalar
    /// field of an owned constructor.
    fn place_native(&self, w: &Walk<'_, 'a>, place: Place) -> Option<Native> {
        let Some((_, Step::Field(c, i))) = &w.graph.places[place] else {
            return None;
        };
        let held = self.own_held(c)?;
        held.has_scalars().then(|| held.field_native(*i)).flatten()
    }

    /// The labels of the record that lies in a value of its constructor,
    /// when `place` is that record.
    fn held_record(&mut self, w: &Walk<'_, 'a>, place: Place) -> Option<Vec<String>> {
        let Some((_, Step::Field(c, 0))) = &w.graph.places[place] else {
            return None;
        };
        match self.own_type(c) {
            Some((_, own::Held::Record(labels))) => Some(labels),
            _ => None,
        }
    }

    /// The Go of the value at `place`.
    fn place(&mut self, w: &mut Walk<'_, 'a>, place: Place) -> Document<'a> {
        if let Some(go) = w.places[place] {
            return go;
        }
        let a = self.a;
        let (from, step) = w.graph.places[place].expect("a part of the scrutinee");
        let go = match &step {
            Step::Field(c, i) => {
                let of = self.place(w, from);
                match self.own_type(c) {
                    Some((ty, held)) => {
                        let ty = a.alloc_str(&ty);
                        match held {
                            own::Held::Zero => {
                                unreachable!("a zero-size constructor has no fields")
                            }
                            own::Held::Record(_) => {
                                docvec![a, "flickrt.Value(&", of, ".(*", ty, ").rec)"]
                            }
                            own::Held::Fields(ref slots) if slots.iter().any(Option::is_some) => {
                                docvec![a, of, ".(*", ty, ").f", *i]
                            }
                            own::Held::Fields(_) => docvec![a, of, ".(*", ty, ").f[", *i, "]"],
                        }
                    }
                    None => call(a, "flickrt.Get", [of, i.to_doc(a)]),
                }
            }
            Step::Item(i) => {
                let of = self.place(w, from);
                call(a, "flickrt.At", [of, i.to_doc(a)])
            }
            Step::Rest(n) => {
                let of = self.place(w, from);
                call(a, "flickrt.Rest", [of, n.to_doc(a)])
            }
            Step::Label(label) => self.label(w, from, label),
        };
        w.places[place] = Some(go);
        go
    }

    /// The Go of the field `label` of the record at `record`: a place in a
    /// value of its constructor when the record lies in one.
    fn label(&mut self, w: &mut Walk<'_, 'a>, record: Place, label: &str) -> Document<'a> {
        let a = self.a;
        if let Some((value, Step::Field(c, 0))) = w.graph.places[record]
            && let Some((ty, own::Held::Record(labels))) = self.own_type(c)
            && let Some(at) = labels.iter().position(|l| l == label)
        {
            let of = self.place(w, value);
            let ty = a.alloc_str(&ty);
            return docvec![a, of, ".(*", ty, ").f[", at, "]"];
        }
        let of = self.place(w, record);
        call(a, "flickrt.Field", [of, self.string(label)])
    }

    /// The Go condition of `test` of the value at `place`.
    fn condition(&mut self, w: &mut Walk<'_, 'a>, place: Place, test: &Test) -> Document<'a> {
        let a = self.a;
        let subject = self.place(w, place);
        match test {
            Test::Tag(c) => {
                if let Some((ty, _)) = self.own_type(c) {
                    // a Go type of its own, which is its tag
                    return call(a, a.alloc_str(&own::test(&ty)), [subject]);
                }
                let tag = self.string(&self.tag(c));
                // of a type whose other constructors may have Go types of their own
                let is = if self.input.own_module(&c.module).is_some() {
                    "flickrt.IsData"
                } else {
                    "flickrt.IsTag"
                };
                call(a, is, [subject, tag])
            }
            Test::Class(c) => {
                let class = match c.rep {
                    flick_ir::Rep::Typeof(class) => class,
                    _ => JsClass::Array,
                };
                call(a, "flickrt.IsClass", [subject, self.string(class.name())])
            }
            Test::Enum(c) => {
                let flick_ir::Rep::Literal(l) = &c.rep else {
                    unreachable!("a constructor that is its literal")
                };
                let lit = a.text(a.alloc_str(&enum_literal(l)));
                call(a, "flickrt.Same", [subject, lit])
            }
            // a `BigInt` is equal by its number, any other literal as Go
            // compares a value with one: by its type, then by what it holds
            Test::Lit(lit @ Lit::BigInt(_)) => {
                let lit = self.literal(lit).to_doc(a);
                call(a, "flickrt.Same", [subject, lit])
            }
            Test::Lit(lit) => {
                let mut lit_doc = self.literal(lit).to_doc(a);
                if let Lit::Float(x) = lit
                    && !Native::is_native_float_lit(*x)
                    && self.place_native(w, place) == Some(Native::Float)
                {
                    lit_doc = Native::Float.unbox(a, lit_doc);
                }
                docvec![a, "(", subject, " == ", lit_doc, ")"]
            }
            Test::Len { n, rest } => {
                let op = if *rest { " >= " } else { " == " };
                let len = call(a, "flickrt.Len", [subject]);
                docvec![a, len, op, *n]
            }
        }
    }
}

/// Whether the tests end, when nothing matches, in the failure itself.
fn ends_in_failure(graph: &Graph) -> bool {
    let mut id = graph.root;
    loop {
        match &graph.nodes[id] {
            Node::Switch { default, .. } => id = *default,
            Node::Guard { otherwise, .. } => id = *otherwise,
            Node::Leaf { .. } => return false,
            Node::Fail => return true,
        }
    }
}
