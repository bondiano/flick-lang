//! Stack safety: a function whose tail position calls itself with all its
//! arguments is a `for` loop, and top-level functions that call each other
//! in tail position run as one function looping over a `switch` on the
//! member to run. A jump assigns the next arguments and `continue`s; each
//! iteration binds the parameters afresh, so a closure keeps the values of
//! its own iteration.

use std::collections::HashMap;
use std::mem;

use super::{Gen, indent, names, param_list};
use crate::codegen::tail::{
    Calls, Target, cycles, function, jumpable, self_call, tail_callees, target,
};
use crate::ir::{BinOp, Expr, ExprKind, Item, Lit, Param, Pat};

/// Where a tail call jumps: the members of the loop with their arity, the
/// loop variables, and the local each parameter of the running member
/// binds, if it is a name.
pub(super) struct Tail {
    members: Vec<(Target, usize)>,
    current: usize,
    vars: Vec<String>,
    binds: Vec<Option<String>>,
    /// In a group, the variable naming the member that runs next.
    state: Option<String>,
}

/// Top-level functions of a module that call each other in tail position.
pub(super) struct Group {
    /// The Go function running the group.
    name: String,
    /// The members, by the index of their item.
    items: Vec<usize>,
    /// The most parameters a member takes.
    arity: usize,
}

/// The groups of `items`, and the group and member of each grouped
/// function by name.
pub(super) fn groups(
    module: &str,
    items: &[Item],
) -> (Vec<Group>, HashMap<String, (usize, usize)>) {
    let mut grouped = HashMap::new();
    let groups = cycles(module, items, Calls::Direct)
        .into_iter()
        .enumerate()
        .map(|(g, members)| {
            let name_of = |i: usize| function(&items[i], Calls::Direct).map_or("", |(n, ..)| n);
            for (k, &i) in members.iter().enumerate() {
                grouped.insert(name_of(i).to_owned(), (g, k));
            }
            let arity = members
                .iter()
                .filter_map(|&i| function(&items[i], Calls::Direct))
                .map(|(_, params, _)| params.len())
                .max()
                .unwrap_or(0);
            Group {
                name: names::group(module, name_of(members[0])),
                items: members,
                arity,
            }
        })
        .collect();
    (groups, grouped)
}

impl Group {
    /// The item of the first member.
    pub(super) fn first(&self) -> usize {
        self.items[0]
    }
}

fn binds(params: &[Param]) -> Vec<Option<String>> {
    params
        .iter()
        .map(|p| match &p.pat {
            Pat::Bind(n) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

impl Gen<'_> {
    /// The Go parameters and statements of a function of `params` running
    /// `body`; with `target`, a self tail call of a pure function jumps.
    pub(super) fn function_body(
        &mut self,
        params: &[Param],
        body: &Expr,
        target: Option<Target>,
    ) -> (Vec<String>, Vec<String>) {
        let saved = (self.tail.take(), mem::replace(&mut self.generator, false));
        let looping = target.filter(|t| {
            jumpable(params, Calls::Direct)
                && !tail_callees(body, &[(t.clone(), params.len())], Calls::Direct).is_empty()
        });
        let result = match looping {
            None => {
                let (stmts, heads) = self.block(|g| {
                    let heads = g.bind_params(params);
                    g.tail_expr(body);
                    heads
                });
                (heads, stmts)
            }
            Some(target) => {
                let heads: Vec<String> = params.iter().map(|_| self.temp()).collect();
                let vars: Vec<String> = params.iter().map(|_| self.temp()).collect();
                let tail = Tail {
                    members: vec![(target, params.len())],
                    current: 0,
                    vars: vars.clone(),
                    binds: binds(params),
                    state: None,
                };
                let (inner, ()) = self.block(|g| g.iteration(params, &vars, body, tail));
                let copies = format!("var {} = {}", param_list(&vars), heads.join(", "));
                (heads, vec![copies, format!("for {{\n{}}}", indent(&inner))])
            }
        };
        (self.tail, self.generator) = saved;
        result
    }

    /// One iteration of a loop: the parameters bound from the loop
    /// variables, then the body in tail position.
    fn iteration(&mut self, params: &[Param], vars: &[String], body: &Expr, tail: Tail) {
        for (p, var) in params.iter().zip(vars) {
            match &p.pat {
                Pat::Bind(n) => self.bind(n, var),
                Pat::Wild | Pat::Lit(Lit::Unit) => {}
                pat => self.destructure(pat, var),
            }
        }
        self.tail = Some(tail);
        self.tail_expr(body);
        self.tail = None;
    }

    /// The body of a grouped top-level function: a call of its group.
    pub(super) fn group_call(&self, name: &str, heads: &[String]) -> Option<String> {
        let &(g, k) = self.grouped.get(name)?;
        let group = &self.groups[g];
        let mut args = vec![k.to_string()];
        args.extend(heads.iter().cloned());
        args.resize(group.arity + 1, "nil".to_owned());
        Some(format!("return {}({})", group.name, args.join(", ")))
    }

    /// The Go function running the group `g` of `items`.
    pub(super) fn group(&mut self, g: usize, items: &[Item]) -> String {
        let functions: Vec<(&str, &[Param], &Expr)> = self.groups[g]
            .items
            .iter()
            .filter_map(|&i| function(&items[i], Calls::Direct))
            .collect();
        let members: Vec<(Target, usize)> = functions
            .iter()
            .map(|(name, params, _)| (target(self.module, name), params.len()))
            .collect();
        let vars: Vec<String> = (0..self.groups[g].arity).map(|i| format!("v{i}")).collect();
        let mut cases = Vec::new();
        for (k, (_, params, body)) in functions.iter().enumerate() {
            let tail = Tail {
                members: members.clone(),
                current: k,
                vars: vars.clone(),
                binds: binds(params),
                state: Some("state".to_owned()),
            };
            let (stmts, ()) = self.block(|g| g.iteration(params, &vars, body, tail));
            cases.push(format!("case {k}:\n{}", indent(&stmts)));
        }
        let mut heads = vec!["state int".to_owned()];
        if !vars.is_empty() {
            heads.push(param_list(&vars));
        }
        format!(
            "func {}({}) flickrt.Value {{\n\tfor {{\n\t\tswitch state {{\n{}\t\t}}\n\t}}\n}}",
            self.groups[g].name,
            heads.join(", "),
            indent(&[indent(&cases)])
        )
    }

    /// Emits a tail call of a member of the loop as a jump, if `e` is one;
    /// whether it was.
    pub(super) fn jump(&mut self, e: &Expr) -> bool {
        let Some((j, args)) = self
            .tail
            .as_ref()
            .and_then(|t| self_call(e, &t.members, Calls::Direct))
        else {
            return false;
        };
        let tail = self.tail.as_ref().expect("a jump is inside a loop");
        let (vars, binds) = (tail.vars.clone(), tail.binds.clone());
        let state = tail.state.clone().filter(|_| j != tail.current);
        // a parameter passed on unchanged keeps its variable
        let changed: Vec<usize> = (0..args.len())
            .filter(|&i| {
                !matches!(&args[i].kind, ExprKind::Local(n)
                    if binds.get(i).is_some_and(|b| b.as_ref() == Some(n)))
            })
            .collect();
        let values = self.ordered(&changed.iter().map(|&i| args[i]).collect::<Vec<_>>());
        if !changed.is_empty() {
            let targets: Vec<&str> = changed.iter().map(|&i| vars[i].as_str()).collect();
            self.buf
                .push(format!("{} = {}", targets.join(", "), values.join(", ")));
        }
        if let Some(state) = state {
            self.buf.push(format!("{state} = {j}"));
        }
        self.buf.push("continue".to_owned());
        true
    }

    /// Whether the right operand of `&&` or `||` in tail position jumps,
    /// which needs the operator as a branch.
    pub(super) fn jumps_in(&self, rhs: &Expr) -> bool {
        self.tail
            .as_ref()
            .is_some_and(|t| !tail_callees(rhs, &t.members, Calls::Direct).is_empty())
    }

    /// `lhs && rhs` or `lhs || rhs` in tail position, its right operand a
    /// jump: the left one decides first.
    pub(super) fn tail_logic(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) {
        let l = self.expr(lhs);
        let (test, value) = if op == BinOp::And {
            (format!("!flickrt.Bool({l})"), "false")
        } else {
            (format!("flickrt.Bool({l})"), "true")
        };
        self.buf.push(format!("if {test} {{\n\treturn {value}\n}}"));
        self.tail_expr(rhs);
    }
}
