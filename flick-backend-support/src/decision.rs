//! The decision graph of a `match`: the tests its arms make of the
//! scrutinee, each made once for every arm that needs it, then the guard
//! and the body of the first arm that matches. Both emitters walk it.
//!
//! - A test reads the scrutinee, already evaluated, or a part of it, and
//!   shows nothing, so tests are free to come in any order and to be shared.
//! - Arms are tried in the order they are written. A guard runs only once
//!   the pattern of its arm matched and every arm before failed; it runs at
//!   most once, and no guard of a later arm runs once an arm is taken. A
//!   guard that fails goes on to the arms after its own.
//! - The names of an arm are bound to the parts the first of its
//!   alternatives that matches gives, before its guard.
//!
//! The graph is a tree whose leaves may name the same arm more than once:
//! a pattern that accepts anything where others test, or an or-pattern,
//! puts its arm on several branches. An emitter writes such a body once and
//! goes to it. A match whose graph would be too big has none: the emitter
//! tries its arms one by one instead.

use std::mem;

use crate::ir::{Arm, Constructor, Lit, Pat, Rep};
use tests_of::{exclusive, parts, same, same_step, test};

/// A part of the scrutinee, by its index in [`Graph::places`].
pub type Place = usize;

/// A node, by its index in [`Graph::nodes`].
pub type NodeId = usize;

/// Tests at places that must all hold, and the node they lead to (see
/// [`Graph::chain`]).
pub type Chain<'g, 'p> = (Vec<(Place, &'g [Test<'p>])>, NodeId);

/// How a place is reached from an earlier one.
#[derive(Debug, Clone, Copy)]
pub enum Step<'p> {
    /// The field `i` of a value of a tagged constructor; past its arity, a
    /// dictionary the value holds.
    Field(&'p Constructor, usize),
    /// The item `i` of a tuple or a list.
    Item(usize),
    /// The items of a list after the first `n`.
    Rest(usize),
    /// A field of a record.
    Label(&'p str),
}

/// What a node tests of the value at a place.
#[derive(Debug, Clone, Copy)]
pub enum Test<'p> {
    /// The tag of a tagged constructor.
    Tag(&'p Constructor),
    /// The JS class of a constructor of an `untagged` type.
    Class(&'p Constructor),
    /// A constructor whose value is its literal.
    Enum(&'p Constructor),
    /// A literal, not `()`.
    Lit(&'p Lit),
    /// A list of `n` items, or with `rest` of at least `n`.
    Len { n: usize, rest: bool },
}

#[derive(Debug, Clone)]
pub enum Node<'p> {
    /// The node of the case one of whose tests holds of the value at
    /// `place`, otherwise `default`. No two tests of a switch both hold.
    Switch {
        place: Place,
        cases: Vec<(Vec<Test<'p>>, NodeId)>,
        default: NodeId,
    },
    /// The pattern of `arm` matched, binding `binds`: its body runs when its
    /// guard holds, and `otherwise` when it does not.
    Guard {
        arm: usize,
        binds: Vec<(&'p str, Place)>,
        otherwise: NodeId,
    },
    /// `arm` matched, binding `binds`: its body runs.
    Leaf {
        arm: usize,
        binds: Vec<(&'p str, Place)>,
    },
    /// No arm matched.
    Fail,
}

/// The decision graph of the arms it borrows its names and tests from.
#[derive(Debug, Clone)]
pub struct Graph<'p> {
    /// How each place is reached; place 0, the scrutinee, from nothing.
    pub places: Vec<Option<(Place, Step<'p>)>>,
    pub nodes: Vec<Node<'p>>,
    pub root: NodeId,
}

impl<'p> Graph<'p> {
    /// How many times an emitter writes a leaf or a guard of each of
    /// `arms` arms, walking the graph as [`Graph::elided`] says: an arm
    /// written more than once has a body of its own to go to.
    pub fn reached(&self, arms: usize) -> Vec<usize> {
        let mut reached = vec![0; arms];
        let mut pending = vec![(self.root, None)];
        while let Some((id, fall)) = pending.pop() {
            if self.elided(id, fall) {
                continue;
            }
            match &self.nodes[id] {
                Node::Switch { cases, default, .. } => {
                    for (_, next) in cases {
                        pending.push((*next, Some(*default)));
                    }
                    pending.push((*default, fall));
                }
                Node::Guard { arm, otherwise, .. } => {
                    reached[*arm] += 1;
                    pending.push((*otherwise, fall));
                }
                Node::Leaf { arm, .. } => reached[*arm] += 1,
                Node::Fail => {}
            }
        }
        reached
    }

    /// Whether an emitter writes nothing for node `id`, the same leaf, or
    /// the failure, as `fall`: the node written where the code goes when
    /// it leaves the block of `id`. An emitter writes the cases of a switch
    /// in blocks, each falling to its default, which follows them, and the
    /// default of a switch, the otherwise of a guard, falling to what its
    /// switch or guard falls to; the tests of the cases after one that
    /// held do not hold.
    pub fn elided(&self, id: NodeId, fall: Option<NodeId>) -> bool {
        fall.is_some_and(|f| same_end(&self.nodes[id], &self.nodes[f]))
    }

    /// The node of a case falling to `fall`, after the run of switches of
    /// one case each from `id` whose defaults are `fall`, written as one
    /// condition with the case's own tests: `(Some a, Some b)` before `_`
    /// is one test of both tags. The tests of the run, in order, and that
    /// node.
    pub fn chain(&self, mut id: NodeId, fall: NodeId) -> Chain<'_, 'p> {
        let mut tests = Vec::new();
        while let Node::Switch {
            place,
            cases,
            default,
        } = &self.nodes[id]
            && let [(more, next)] = &cases[..]
            && self.elided(*default, Some(fall))
        {
            tests.push((*place, &more[..]));
            id = *next;
        }
        (tests, id)
    }
}

/// Whether two nodes are one leaf, or both the failure: an emitter writes
/// them alike.
fn same_end(a: &Node<'_>, b: &Node<'_>) -> bool {
    match (a, b) {
        (Node::Leaf { arm: x, binds: bx }, Node::Leaf { arm: y, binds: by }) => x == y && bx == by,
        (Node::Fail, Node::Fail) => true,
        _ => false,
    }
}

/// The decision graph of `arms`, or `None` when it would have more than a
/// few nodes for each arm: an or-pattern in each of several fields, or
/// many patterns accepting anything where others test, multiply them.
pub fn build(arms: &[Arm]) -> Option<Graph<'_>> {
    let mut builder = Builder {
        arms,
        places: vec![None],
        nodes: Vec::new(),
        budget: 32 + 16 * arms.len(),
    };
    let mut rows = Vec::with_capacity(arms.len());
    for (arm, a) in arms.iter().enumerate() {
        rows.push(Row {
            pats: vec![&a.pat],
            arm,
            binds: Vec::new(),
        });
    }
    let root = builder.compile(vec![0], rows)?;
    Some(Graph {
        places: builder.places,
        nodes: builder.nodes,
        root,
    })
}

static WILD: Pat = Pat::Wild;

/// The patterns an arm has left to match at the places of the columns.
#[derive(Clone)]
struct Row<'p> {
    pats: Vec<&'p Pat>,
    arm: usize,
    binds: Vec<(&'p str, Place)>,
}

struct Builder<'p> {
    arms: &'p [Arm],
    places: Vec<Option<(Place, Step<'p>)>>,
    nodes: Vec<Node<'p>>,
    /// How many more nodes the graph may have.
    budget: usize,
}

impl<'p> Builder<'p> {
    fn push(&mut self, node: Node<'p>) -> Option<NodeId> {
        self.budget = self.budget.checked_sub(1)?;
        self.nodes.push(node);
        Some(self.nodes.len() - 1)
    }

    /// The place `step` leads to from `from`, the same for every arm.
    fn place(&mut self, from: Place, step: Step<'p>) -> Place {
        let found = self.places.iter().position(|p| match p {
            Some((f, s)) => *f == from && same_step(*s, step),
            None => false,
        });
        if let Some(at) = found {
            return at;
        }
        self.places.push(Some((from, step)));
        self.places.len() - 1
    }

    /// The node that matches `rows`, in order, on the values at `cols`.
    fn compile(&mut self, mut cols: Vec<Place>, mut rows: Vec<Row<'p>>) -> Option<NodeId> {
        self.simplify(&mut cols, &mut rows)?;
        let Some(first) = rows.first() else {
            return self.push(Node::Fail);
        };
        let Some(col) = first.pats.iter().position(|p| !matches!(p, Pat::Wild)) else {
            let mut rest = rows.into_iter();
            let row = rest.next().expect("a first row");
            if self.arms[row.arm].guard.is_none() {
                return self.push(Node::Leaf {
                    arm: row.arm,
                    binds: row.binds,
                });
            }
            // the other alternatives of an or-pattern would run the guard again
            let mut others = Vec::with_capacity(rest.len());
            for r in rest {
                if r.arm != row.arm {
                    others.push(r);
                }
            }
            let otherwise = self.compile(cols, others)?;
            return self.push(Node::Guard {
                arm: row.arm,
                binds: row.binds,
                otherwise,
            });
        };
        let place = cols[col];
        let mut chosen: Vec<Test<'p>> = Vec::new();
        for row in &rows {
            if let Some(t) = test(row.pats[col])
                && chosen.iter().all(|c| exclusive(*c, t))
            {
                chosen.push(t);
            }
        }
        let mut cases: Vec<(Vec<Test<'p>>, NodeId)> = Vec::new();
        for &case in &chosen {
            let mut width = 0;
            for row in &rows {
                if test(row.pats[col]).is_some_and(|t| same(t, case)) {
                    width = width.max(parts(row.pats[col]).len());
                }
            }
            let mut subplaces = Vec::with_capacity(width);
            for i in 0..width {
                subplaces.push(self.subplace(place, case, i));
            }
            let sub_cols = spliced(&cols, col + 1, 0, &subplaces);
            let mut sub_rows = Vec::with_capacity(rows.len());
            for row in &rows {
                if let Some(r) = specialize(row, col, case, width) {
                    sub_rows.push(r);
                }
            }
            let node = self.compile(sub_cols, sub_rows)?;
            // cases that end in one arm binding nothing are one case: the
            // alternatives of `| B | KB -> 0`
            let leaf = matches!(&self.nodes[node], Node::Leaf { binds, .. } if binds.is_empty());
            let merged = cases
                .iter()
                .position(|(_, other)| leaf && same_end(&self.nodes[*other], &self.nodes[node]));
            match merged {
                Some(at) => {
                    cases[at].0.push(case);
                    self.nodes.pop();
                    self.budget += 1;
                }
                None => cases.push((vec![case], node)),
            }
        }
        let mut rest = Vec::with_capacity(rows.len());
        for r in rows {
            if test(r.pats[col]).is_none_or(|t| !chosen.iter().any(|c| same(*c, t))) {
                rest.push(r);
            }
        }
        let default = self.compile(cols, rest)?;
        self.push(Node::Switch {
            place,
            cases,
            default,
        })
    }

    /// The place of the part `i` of a value `test` holds of the value at
    /// `place`: the value itself for the one field of a constructor told
    /// apart by its JS class.
    fn subplace(&mut self, place: Place, test: Test<'p>, i: usize) -> Place {
        match test {
            Test::Tag(c) => self.place(place, Step::Field(c, i)),
            Test::Len { n, .. } if i == n => self.place(place, Step::Rest(n)),
            Test::Len { .. } => self.place(place, Step::Item(i)),
            Test::Class(_) | Test::Enum(_) | Test::Lit(_) => place,
        }
    }

    /// Binds names, makes an arm a row per alternative of its or-patterns,
    /// takes tuples and records apart into a column per part, and drops the
    /// columns no row tests; `None` when there are too many rows.
    fn simplify(&mut self, cols: &mut Vec<Place>, rows: &mut Vec<Row<'p>>) -> Option<()> {
        let mut col = 0;
        while col < cols.len() {
            if rows.iter().any(|r| matches!(r.pats[col], Pat::Or(_))) {
                *rows = alternatives(mem::take(rows), col);
                if rows.len() > self.budget {
                    return None;
                }
                continue;
            }
            let mut revealed = false;
            for row in rows.iter_mut() {
                loop {
                    match row.pats[col] {
                        Pat::Bind(name) => row.binds.push((name, cols[col])),
                        Pat::Lit(Lit::Unit) => {}
                        // the value of the constructor is its field
                        Pat::Con(c, args) if c.rep == Rep::Unboxed => {
                            row.pats[col] = args.first().unwrap_or(&WILD);
                            revealed = true;
                            continue;
                        }
                        _ => break,
                    }
                    row.pats[col] = &WILD;
                }
            }
            if revealed {
                continue;
            }
            let mut steps: Vec<Step<'p>> = Vec::new();
            for row in rows.iter() {
                match row.pats[col] {
                    Pat::Tuple(items) if steps.is_empty() => {
                        steps.extend((0..items.len()).map(Step::Item));
                    }
                    Pat::Record(fields) => {
                        for (label, _) in fields {
                            if !steps
                                .iter()
                                .any(|s| matches!(s, Step::Label(l) if l == label))
                            {
                                steps.push(Step::Label(label));
                            }
                        }
                    }
                    _ => {}
                }
            }
            if steps.is_empty() {
                if rows.iter().all(|r| matches!(r.pats[col], Pat::Wild)) {
                    *cols = spliced(cols, col, 1, &[]);
                    for row in rows.iter_mut() {
                        row.pats = spliced(&row.pats, col, 1, &[]);
                    }
                } else {
                    col += 1;
                }
                continue;
            }
            // a tuple or a record: a column per part
            let mut parts = Vec::with_capacity(steps.len());
            for &step in &steps {
                parts.push(self.place(cols[col], step));
            }
            *cols = spliced(cols, col, 1, &parts);
            for row in rows.iter_mut() {
                let mut split = vec![&WILD; steps.len()];
                for (at, step) in steps.iter().enumerate() {
                    split[at] = match (row.pats[col], step) {
                        (Pat::Tuple(items), Step::Item(i)) => &items[*i],
                        (Pat::Record(fields), Step::Label(l)) => fields
                            .iter()
                            .find(|(f, _)| f == l)
                            .map_or(&WILD, |(_, p)| p),
                        _ => &WILD,
                    };
                }
                row.pats = spliced(&row.pats, col, 1, &split);
            }
        }
        Some(())
    }
}

/// `items` with `count` of them from `at` replaced with `with`.
fn spliced<T: Copy>(items: &[T], at: usize, count: usize, with: &[T]) -> Vec<T> {
    let mut out = Vec::with_capacity(items.len() + with.len());
    out.extend_from_slice(&items[..at]);
    out.extend_from_slice(with);
    out.extend_from_slice(&items[at + count..]);
    out
}

/// `rows` with each or-pattern in column `col` an alternative per row, in
/// order: the first that matches binds the names.
fn alternatives(rows: Vec<Row<'_>>, col: usize) -> Vec<Row<'_>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        flatten(row, col, &mut out);
    }
    out
}

fn flatten<'p>(row: Row<'p>, col: usize, out: &mut Vec<Row<'p>>) {
    let Pat::Or(alts) = row.pats[col] else {
        out.push(row);
        return;
    };
    for alt in alts {
        let mut r = row.clone();
        r.pats[col] = alt;
        flatten(r, col, out);
    }
}

/// `row` where the value at `col` passed `case`, whose parts are the
/// `width` columns after it; `None` when the row cannot match there.
fn specialize<'p>(row: &Row<'p>, col: usize, case: Test<'p>, width: usize) -> Option<Row<'p>> {
    let mut subs = vec![&WILD; width];
    let mut row = row.clone();
    match test(row.pats[col]) {
        Some(t) if same(t, case) => {
            for (at, part) in parts(row.pats[col]).into_iter().enumerate() {
                subs[at] = part;
            }
            row.pats[col] = &WILD;
        }
        Some(t) if exclusive(case, t) => return None,
        // a pattern accepting anything, or a test that may hold too, stays
        _ => {}
    }
    row.pats = spliced(&row.pats, col + 1, 0, &subs);
    Some(row)
}

mod tests_of;

#[cfg(test)]
mod tests;
