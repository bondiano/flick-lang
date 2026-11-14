//! Tail calls, as every backend finds them: the full pure calls in tail
//! position of a function of itself or of the members of its group, and the
//! groups of top-level functions that call each other in tail position. A
//! backend makes them jumps, so recursion through them needs no stack.

use flick_ir::Dict;
use flick_ir::{BinOp, Expr, ExprKind, Item, ItemKind, Param, Pat, Stage};

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

/// A top-level function a tail call may jump to: its name, parameters and
/// body.
pub type Function<'i> = (&'i str, &'i [Param], &'i Expr);

/// The functions of the items `group` of `items`, and the target and arity
/// of each, which a tail call jumps to.
pub fn members<'i>(
    module: &str,
    items: &'i [Item],
    group: &[usize],
    calls: Calls,
) -> (Vec<Function<'i>>, Vec<(Target, usize)>) {
    let functions: Vec<Function> = (group.iter())
        .filter_map(|&i| function(&items[i], calls))
        .collect();
    let members = (functions.iter())
        .map(|(name, params, _)| (target(module, name), params.len()))
        .collect();
    (functions, members)
}

/// The local each parameter binds, if it is a name.
pub fn binds(params: &[Param]) -> Vec<Option<String>> {
    (params.iter())
        .map(|p| match &p.pat {
            Pat::Bind(n) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

/// Whether the argument `i` of a jump is the local the parameter `i` binds:
/// passed on unchanged, it keeps its loop variable.
pub fn passed_on(args: &[&Expr], binds: &[Option<String>], i: usize) -> bool {
    matches!(&args[i].kind, ExprKind::Local(n) if binds.get(i).is_some_and(|b| b.as_ref() == Some(n)))
}

/// Whether a tail position of `body` calls one of `members` in full.
pub fn has_tail_call(body: &Expr, members: &[(Target, usize)], calls: Calls) -> bool {
    !tail_callees(body, members, calls).is_empty()
}

/// The cycles of tail calls between the functions of `items`, by item
/// index in order, found as strongly connected components (Tarjan).
pub fn cycles(module: &str, items: &[Item], calls: Calls) -> Vec<Vec<usize>> {
    let indices: Vec<usize> = (0..items.len())
        .filter(|&i| function(&items[i], calls).is_some())
        .collect();
    let (functions, members) = members(module, items, &indices, calls);
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
            c.iter().map(|&f| indices[f]).collect()
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
    let (mut head, spine) = e.spine();
    let mut args = Vec::with_capacity(spine.len());
    for (i, &(arg, stage)) in spine.iter().enumerate() {
        // the type of the callee fixes the stage of a full call's last arrow
        let last = i + 1 == spine.len() && calls == Calls::Direct;
        if stage != Stage::Pure && !last {
            return None;
        }
        args.push(arg);
    }
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
