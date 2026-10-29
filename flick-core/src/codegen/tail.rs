//! Tail calls, as every backend finds them: the full pure calls in tail
//! position of a function of itself or of the members of its group, and the
//! groups of top-level functions that call each other in tail position. A
//! backend makes them jumps, so recursion through them needs no stack.

use crate::ir::{BinOp, Expr, ExprKind, Item, ItemKind, Param, Stage};
use crate::types::Dict;

/// A function a tail call may jump to: its name and the dictionaries of its
/// constraints, by the names `Dict::Given` gives them, which a call passes
/// on.
#[derive(Clone, PartialEq, Eq)]
pub struct Target {
    pub callee: Callee,
    pub dicts: Vec<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum Callee {
    Local(String),
    Global(String),
}

/// Which calls a backend can jump for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Calls {
    /// Pure calls only: a runtime call of the JS target runs in a generator.
    Pure,
    /// Runtime and Sync calls too, whose last stage is a direct call, as on
    /// the go target.
    Direct,
}

/// The target of the top-level function `name` of `module`.
pub fn target(module: &str, name: &str) -> Target {
    Target {
        callee: Callee::Global(format!("{module}.{name}")),
        dicts: Vec::new(),
    }
}

/// The parameters and body of a top-level function without dictionaries
/// whose stages are pure, but for the last one with `Calls::Direct`.
pub fn function(item: &Item, calls: Calls) -> Option<(&str, &[Param], &Expr)> {
    match &item.kind {
        ItemKind::Value {
            name,
            dicts,
            body:
                Expr {
                    kind: ExprKind::Lambda { params, body },
                    ..
                },
        } if dicts.is_empty() && jumpable(params, calls) => Some((name, params, body)),
        _ => None,
    }
}

/// Whether a function of `params` can loop on its tail calls.
pub fn jumpable(params: &[Param], calls: Calls) -> bool {
    let before_last = &params[..params.len().saturating_sub(1)];
    match calls {
        Calls::Pure => params.iter().all(|p| p.stage == Stage::Pure),
        Calls::Direct => before_last.iter().all(|p| p.stage == Stage::Pure),
    }
}

/// The cycles of tail calls between the functions of `items`, by item
/// index in order, found as strongly connected components (Tarjan).
pub fn cycles(module: &str, items: &[Item], calls: Calls) -> Vec<Vec<usize>> {
    let functions: Vec<(usize, &str, &[Param], &Expr)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| function(item, calls).map(|(n, p, b)| (i, n, p, b)))
        .collect();
    let members: Vec<(Target, usize)> = functions
        .iter()
        .map(|(_, name, params, _)| (target(module, name), params.len()))
        .collect();
    let edges: Vec<Vec<usize>> = functions
        .iter()
        .map(|(.., body)| tail_callees(body, &members, calls))
        .collect();
    let mut scc = Scc {
        edges: &edges,
        index: vec![None; edges.len()],
        low: vec![0; edges.len()],
        stack: Vec::new(),
        on_stack: vec![false; edges.len()],
        next: 0,
        out: Vec::new(),
    };
    for v in 0..edges.len() {
        if scc.index[v].is_none() {
            scc.visit(v);
        }
    }
    scc.out
        .into_iter()
        .filter(|c| c.len() > 1)
        .map(|mut c| {
            c.sort_unstable();
            c.iter().map(|&f| functions[f].0).collect()
        })
        .collect()
}

struct Scc<'a> {
    edges: &'a [Vec<usize>],
    index: Vec<Option<usize>>,
    low: Vec<usize>,
    stack: Vec<usize>,
    on_stack: Vec<bool>,
    next: usize,
    out: Vec<Vec<usize>>,
}

impl Scc<'_> {
    fn visit(&mut self, v: usize) {
        self.index[v] = Some(self.next);
        self.low[v] = self.next;
        self.next += 1;
        self.stack.push(v);
        self.on_stack[v] = true;
        for &w in &self.edges[v] {
            match self.index[w] {
                None => {
                    self.visit(w);
                    self.low[v] = self.low[v].min(self.low[w]);
                }
                Some(i) if self.on_stack[w] => self.low[v] = self.low[v].min(i),
                Some(_) => {}
            }
        }
        if Some(self.low[v]) == self.index[v] {
            let mut component = Vec::new();
            while let Some(w) = self.stack.pop() {
                self.on_stack[w] = false;
                component.push(w);
                if w == v {
                    break;
                }
            }
            self.out.push(component);
        }
    }
}

/// The member a full call of one of `members` calls, and its arguments:
/// a pure one, or with `Calls::Direct` one whose last stage may run.
pub fn self_call<'e>(
    e: &'e Expr,
    members: &[(Target, usize)],
    calls: Calls,
) -> Option<(usize, Vec<&'e Expr>)> {
    let mut args = Vec::new();
    let mut head = e;
    while let ExprKind::App {
        func,
        arg,
        stage,
        arg_first: false,
    } = &head.kind
    {
        // the type of the callee fixes the stage of a full call's last arrow
        let last = args.is_empty() && calls == Calls::Direct;
        if *stage != Stage::Pure && !last {
            return None;
        }
        args.push(&**arg);
        head = func;
    }
    args.reverse();
    let mut dicts = Vec::new();
    if let ExprKind::Dicts(inner, given) = &head.kind {
        for d in given {
            match d {
                Dict::Given(name) => dicts.push(name.clone()),
                _ => return None,
            }
        }
        head = inner;
    }
    let callee = match &head.kind {
        ExprKind::Local(name) => Callee::Local(name.clone()),
        ExprKind::Global { module, name } => Callee::Global(format!("{module}.{name}")),
        _ => return None,
    };
    let target = Target { callee, dicts };
    let j = members
        .iter()
        .position(|(t, arity)| *t == target && *arity == args.len())?;
    Some((j, args))
}

/// The members of `members` a tail position of `body` calls in full.
pub fn tail_callees(body: &Expr, members: &[(Target, usize)], calls: Calls) -> Vec<usize> {
    let mut out = Vec::new();
    let mut pending = vec![body];
    while let Some(e) = pending.pop() {
        match &e.kind {
            ExprKind::Block(_, result) => pending.push(result),
            ExprKind::If(_, t, f) => pending.extend([&**t, &**f]),
            ExprKind::Match(_, arms) => pending.extend(arms.iter().map(|a| &a.body)),
            ExprKind::Binary(BinOp::And | BinOp::Or, _, r) => pending.push(r),
            _ => out.extend(self_call(e, members, calls).map(|(j, _)| j)),
        }
    }
    out
}
