//! Top-level functions that call each other in tail position run as one
//! loop: a function of the member to run and the loop variables, switching
//! on the member. Each member calls it with its own index; a tail call of
//! another member sets the variables and the member and jumps.

use std::collections::HashMap;
use std::mem;

use super::function::{binds, offsets, tail_callees};
use super::{Callee, Gen, Tail, Target, indent, names, view};
use crate::ir::{Expr, ExprKind, Item, ItemKind, Lit, Param, Pat, Stage};

/// Mutually tail-recursive functions of a module.
pub(super) struct Group {
    /// The JS name of the function running the group.
    pub(super) name: String,
    /// The members, by the index of their item.
    pub(super) items: Vec<usize>,
}

/// The parameters and body of a top-level pure function without
/// dictionaries.
fn function(item: &Item) -> Option<(&str, &[Param], &Expr)> {
    match &item.kind {
        ItemKind::Value {
            name,
            dicts,
            body:
                Expr {
                    kind: ExprKind::Lambda { params, body },
                    ..
                },
        } if dicts.is_empty() && params.iter().all(|p| p.stage == Stage::Pure) => {
            Some((name, params, body))
        }
        _ => None,
    }
}

fn target(module: &str, name: &str) -> Target {
    Target {
        callee: Callee::Global(format!("{module}.{name}")),
        dicts: Vec::new(),
    }
}

/// The groups of `items`: the cycles of tail calls between its functions,
/// found as strongly connected components (Tarjan).
pub(super) fn groups(module: &str, items: &[Item]) -> Vec<Group> {
    let functions: Vec<(usize, &str, &[Param], &Expr)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| function(item).map(|(n, p, b)| (i, n, p, b)))
        .collect();
    let members: Vec<(Target, usize)> = functions
        .iter()
        .map(|(_, name, params, _)| (target(module, name), params.len()))
        .collect();
    let edges: Vec<Vec<usize>> = functions
        .iter()
        .map(|(.., body)| tail_callees(body, &members))
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
            let items: Vec<usize> = c.iter().map(|&f| functions[f].0).collect();
            Group {
                name: format!("{}$rec", names::ident(functions[c[0]].1)),
                items,
            }
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

/// The group and member of each grouped function, by its global name.
pub(super) fn grouped(
    module: &str,
    items: &[Item],
    groups: &[Group],
) -> HashMap<String, (usize, usize)> {
    groups
        .iter()
        .enumerate()
        .flat_map(|(g, group)| {
            group.items.iter().enumerate().filter_map(move |(k, &i)| {
                function(&items[i]).map(|(name, ..)| (format!("{module}.{name}"), (g, k)))
            })
        })
        .collect()
}

impl Gen<'_> {
    /// The body of the member `target`, if it is one: a call of its group.
    pub(super) fn group_call(&self, target: &Target, heads: &[String]) -> Option<String> {
        let Callee::Global(global) = &target.callee else {
            return None;
        };
        let &(g, k) = self.grouped.get(global)?;
        Some(format!(
            "{}({k}, {})",
            self.groups[g].name,
            heads.join(", ")
        ))
    }

    /// The function running the group `g` of `items`.
    pub(super) fn group(&mut self, g: usize, items: &[Item]) -> String {
        let functions: Vec<(&str, &[Param], &Expr)> = self.groups[g]
            .items
            .iter()
            .filter_map(|&i| function(&items[i]))
            .collect();
        let members: Vec<(Target, usize)> = functions
            .iter()
            .map(|(name, params, _)| (target(self.module, name), params.len()))
            .collect();
        let arity = members.iter().map(|(_, n)| *n).max().unwrap_or(0);
        let vars: Vec<String> = (0..arity).map(|i| format!("$l{i}")).collect();
        let bodies: Vec<(&[Param], &Expr)> = functions.iter().map(|(_, p, b)| (*p, *b)).collect();
        let offsets = offsets(&view::positions(&bodies, &members));
        let saved = (mem::replace(&mut self.generator, false), self.tail.take());
        let mut cases = Vec::new();
        for (k, (_, params, body)) in functions.iter().enumerate() {
            let (stmts, ()) = self.block(|g| {
                let mut heads = Vec::new();
                for (p, var) in params.iter().zip(&vars) {
                    match &p.pat {
                        Pat::Bind(n) => {
                            let c = g.constant(&names::ident(n), var);
                            g.buf.push(c);
                        }
                        Pat::Wild | Pat::Lit(Lit::Unit) => {}
                        pat => g.destructure(pat, var),
                    }
                    heads.push(match &p.pat {
                        Pat::Bind(n) => names::ident(n),
                        _ => String::new(),
                    });
                }
                g.walk(&heads, &offsets, |g| {
                    g.tail = Some(Tail {
                        members: members.clone(),
                        current: k,
                        vars: vars.clone(),
                        binds: binds(params),
                        state: Some("$f".to_owned()),
                        offsets: offsets.clone(),
                    });
                    g.tail_expr(body);
                    g.tail = None;
                });
            });
            cases.push(format!("case {k}: {{\n{}}}", indent(&stmts)));
        }
        (self.generator, self.tail) = saved;
        let switch = format!("switch ($f) {{\n{}}}", indent(&cases));
        let mut body = vec![format!("for (;;) {{\n{}}}", indent(&[switch]))];
        let init: Vec<String> = offsets
            .iter()
            .flatten()
            .map(|o| format!("{o} = 0"))
            .collect();
        if !init.is_empty() {
            body.insert(0, format!("let {};", init.join(", ")));
        }
        let params = format!("$f, {}", vars.join(", "));
        let value = format!("{} {{\n{}}}", self.arrow(&params), indent(&body));
        self.constant(&self.groups[g].name, &value)
    }
}
