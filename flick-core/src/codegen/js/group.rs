//! Top-level functions that call each other in tail position run as one
//! loop: a function of the member to run and the loop variables, switching
//! on the member. Each member calls it with its own index; a tail call of
//! another member sets the variables and the member and jumps.

use std::collections::HashMap;
use std::mem;

use super::function::{binds, offsets};
use super::{Gen, Tail, indent, names, view};
use crate::codegen::tail::{Callee, Calls, Target, cycles, function, target};
use crate::ir::{Expr, Item, Lit, Param, Pat};

/// Mutually tail-recursive functions of a module.
pub(super) struct Group {
    /// The JS name of the function running the group.
    pub(super) name: String,
    /// The members, by the index of their item.
    pub(super) items: Vec<usize>,
}

/// The groups of `items`, each named after its first member.
pub(super) fn groups(module: &str, items: &[Item]) -> Vec<Group> {
    cycles(module, items, Calls::Pure)
        .into_iter()
        .map(|items_of| Group {
            name: format!(
                "{}$rec",
                names::ident(function(&items[items_of[0]], Calls::Pure).map_or("", |(n, ..)| n))
            ),
            items: items_of,
        })
        .collect()
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
                function(&items[i], Calls::Pure)
                    .map(|(name, ..)| (format!("{module}.{name}"), (g, k)))
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
            .filter_map(|&i| function(&items[i], Calls::Pure))
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
