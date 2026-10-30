//! Stack safety: a function whose tail position calls itself with all its
//! arguments is a `for` loop, and top-level functions that call each other
//! in tail position run as one function looping over a `switch` on the
//! member to run. A jump assigns the next arguments and `continue`s; each
//! iteration binds the parameters afresh, so a closure keeps the values of
//! its own iteration.

use std::collections::HashMap;
use std::mem;

use flick_pretty::{Document, DocumentArena, Documentable, docvec};

use super::arith::Native;
use super::layout::{Code, INDENT, block, call, param_list, single, statement};
use super::{Gen, names};
use crate::codegen::tail::{
    Calls, Target, cycles, function, jumpable, self_call, tail_callees, target,
};
use crate::ir::{BinOp, Expr, ExprKind, Item, Lit, Param, Pat, Stage};

/// Where a tail call jumps: the members of the loop with their arity, the
/// loop variables, and the local each parameter of the running member
/// binds, if it is a name.
pub(super) struct Tail<'a> {
    members: Vec<(Target, usize)>,
    current: usize,
    vars: Vec<&'a str>,
    binds: Vec<Option<String>>,
    /// The Go type of each loop variable that is not a `flickrt.Value`.
    natives: Vec<Option<Native>>,
    /// In a group, the variable naming the member that runs next.
    state: Option<&'static str>,
}

/// Top-level functions of a module that call each other in tail position.
pub(super) struct Group {
    /// The Go function running the group.
    name: String,
    /// The members, by the index of their item.
    items: Vec<usize>,
    /// The most parameters a member takes.
    arity: usize,
    /// Whether its members run effects, and so take the fiber.
    fiber: bool,
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
            let fiber =
                function(&items[members[0]], Calls::Direct).is_some_and(|(_, params, _)| {
                    params.last().is_some_and(|p| p.stage != Stage::Pure)
                });
            Group {
                name: names::group(module, name_of(members[0])),
                items: members,
                arity,
                fiber,
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

impl<'a> Gen<'a> {
    /// The Go parameters and statements of a function of `params` running
    /// `body`; with `target`, a self tail call of a pure function jumps.
    pub(super) fn function_body(
        &mut self,
        params: &[Param],
        body: &Expr,
        target: Option<Target>,
    ) -> (Vec<&'a str>, Vec<Document<'a>>) {
        let a = self.a;
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
                let heads: Vec<&'a str> = params.iter().map(|_| self.temp()).collect();
                let vars: Vec<&'a str> = params.iter().map(|_| self.temp()).collect();
                // a loop variable of a known scalar type is unboxed once, not per iteration
                let natives: Vec<Option<Native>> = params
                    .iter()
                    .map(|p| {
                        matches!(p.pat, Pat::Bind(_))
                            .then(|| Native::of(p.ty.as_deref()))
                            .flatten()
                    })
                    .collect();
                let tail = Tail {
                    members: vec![(target, params.len())],
                    current: 0,
                    vars: vars.clone(),
                    binds: binds(params),
                    natives: natives.clone(),
                    state: None,
                };
                let (inner, ()) = self.block(|g| g.iteration(params, &vars, body, tail));
                let mut stmts: Vec<Document<'a>> = vars
                    .iter()
                    .zip(&heads)
                    .zip(&natives)
                    .map(|((&var, &head), native)| match native {
                        Some(n) => {
                            let value = n.unbox(a, a.text(head));
                            statement(a, docvec![a, "var ", var, " ", n.go(), " = ", value])
                        }
                        None => statement(a, docvec![a, "var ", var, " flickrt.Value = ", head]),
                    })
                    .collect();
                let body = checkpoint(a, self.fiber).into_iter().chain(inner);
                stmts.push(statement(a, docvec![a, "for ", block(a, body)]));
                (heads, stmts)
            }
        };
        (self.tail, self.generator) = saved;
        result
    }

    /// One iteration of a loop: the parameters bound from the loop
    /// variables, then the body in tail position.
    fn iteration(&mut self, params: &[Param], vars: &[&'a str], body: &Expr, tail: Tail<'a>) {
        for ((p, &var), native) in params.iter().zip(vars).zip(&tail.natives) {
            match (&p.pat, native) {
                (Pat::Bind(n), Some(native)) => self.bind_native(n, self.code(var), *native),
                (Pat::Bind(n), None) => self.bind(n, self.code(var)),
                (Pat::Wild | Pat::Lit(Lit::Unit), _) => {}
                (pat, _) => self.destructure(pat, self.a.text(var)),
            }
        }
        self.tail = Some(tail);
        self.tail_expr(body);
        self.tail = None;
    }

    /// The body of a grouped top-level function: a call of its group.
    pub(super) fn group_call(&self, name: &str, heads: &[&'a str]) -> Option<Document<'a>> {
        let a = self.a;
        let &(g, k) = self.grouped.get(name)?;
        let group = &self.groups[g];
        let mut args = vec![k.to_doc(a)];
        args.extend(heads.iter().map(|h| a.text(h)));
        args.resize(group.arity + 1, a.text("nil"));
        if group.fiber {
            args.insert(0, a.text("fb"));
        }
        let go = call(a, a.alloc_str(&group.name), args);
        Some(statement(a, docvec![a, "return ", go]))
    }

    /// The Go function running the group `g` of `items`.
    pub(super) fn group(&mut self, g: usize, items: &[Item]) -> Document<'a> {
        let a = self.a;
        let functions: Vec<(&str, &[Param], &Expr)> = self.groups[g]
            .items
            .iter()
            .filter_map(|&i| function(&items[i], Calls::Direct))
            .collect();
        let members: Vec<(Target, usize)> = functions
            .iter()
            .map(|(name, params, _)| (target(self.module, name), params.len()))
            .collect();
        let vars: Vec<&'a str> = (0..self.groups[g].arity)
            .map(|i| a.alloc_str(&format!("v{i}")))
            .collect();
        let fiber = self.groups[g].fiber;
        self.fiber = fiber.then_some("fb");
        let mut cases = Vec::new();
        for (k, (_, params, body)) in functions.iter().enumerate() {
            let tail = Tail {
                members: members.clone(),
                current: k,
                vars: vars.clone(),
                binds: binds(params),
                natives: vec![None; params.len()],
                state: Some("state"),
            };
            let (stmts, ()) = self.block(|g| g.iteration(params, &vars, body, tail));
            let stmts = a.concat(stmts).nest(a, INDENT);
            cases.push(statement(a, docvec![a, "case ", k, ":", stmts]));
        }
        self.fiber = None;
        let mut heads = vec!["state int".to_owned()];
        if fiber {
            heads.insert(0, "fb *flickrt.Fiber".to_owned());
        }
        if !vars.is_empty() {
            heads.push(param_list(&vars));
        }
        let switch = docvec![a, "switch state {", a.concat(cases), a.line(), "}"];
        let body = checkpoint(a, fiber.then_some("fb"))
            .into_iter()
            .chain([statement(a, switch)]);
        let body = single(a, docvec![a, "for ", block(a, body)]);
        let name = self.groups[g].name.clone();
        docvec![
            a,
            "func ",
            name,
            "(",
            heads.join(", "),
            ") flickrt.Value ",
            body
        ]
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
        let (vars, binds, natives) = (tail.vars.clone(), tail.binds.clone(), tail.natives.clone());
        let state = tail.state.filter(|_| j != tail.current);
        // a parameter passed on unchanged keeps its variable
        let changed: Vec<usize> = (0..args.len())
            .filter(|&i| {
                !matches!(&args[i].kind, ExprKind::Local(n)
                    if binds.get(i).is_some_and(|b| b.as_ref() == Some(n)))
            })
            .collect();
        let values = self.ordered(&changed.iter().map(|&i| args[i]).collect::<Vec<_>>());
        let values: Vec<Code<'a>> = changed
            .iter()
            .zip(values)
            .map(|(&i, v)| match natives[i] {
                Some(n) => self.unboxed(args[i], v, n),
                None => v,
            })
            .collect();
        if !changed.is_empty() {
            let a = self.a;
            let targets = a.join(changed.iter().map(|&i| a.text(vars[i])), a.text(", "));
            let values = a.join(values.iter().map(|v| v.doc), a.text(", "));
            self.emit(docvec![a, targets, " = ", values]);
        }
        if let Some(state) = state {
            self.emit(docvec![self.a, state, " = ", j]);
        }
        self.emit("continue");
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
        let a = self.a;
        let l = self.expr(lhs);
        let (test, value) = if op == BinOp::And {
            ("!flickrt.Bool", "false")
        } else {
            ("flickrt.Bool", "true")
        };
        let test = call(a, test, [l.doc]);
        self.emit(docvec![
            a,
            "if ",
            test,
            " ",
            single(a, docvec![a, "return ", value])
        ]);
        self.tail_expr(rhs);
    }
}

/// The interruption checkpoint at the head of a loop of effectful code.
fn checkpoint<'a>(a: &'a DocumentArena<'a>, fiber: Option<&'static str>) -> Option<Document<'a>> {
    fiber.map(|fb| statement(a, call(a, "flickrt.Check", [a.text(fb)])))
}
