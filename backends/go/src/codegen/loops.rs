//! Stack safety: a function whose tail position calls itself with all its
//! arguments is a `for` loop, and top-level functions that call each other
//! in tail position run as one function looping over a `switch` on the
//! member to run. A jump assigns the next arguments and `continue`s; each
//! iteration binds the parameters afresh, so a closure keeps the values of
//! its own iteration.

use std::collections::HashMap;
use std::mem;

use flick_pretty::{Document, DocumentArena, Documentable, docvec};

use super::go_type::{GoType, Typed};
use super::layout::{INDENT, block, call, param_list, single, statement};
use super::shape::Types;
use super::{Gen, names};
use flick_backend_support::tail::{
    Calls, Target, binds, cycles, function, has_tail_call, jumpable, members, passed_on, self_call,
};
use flick_ir::{BinOp, Expr, Item, Lit, Param, Pat, Stage};

/// Where a tail call jumps: the members of the loop with their arity, the
/// loop variables, and the local each parameter of the running member
/// binds, if it is a name.
pub(super) struct Tail<'a> {
    members: Vec<(Target, usize)>,
    current: usize,
    vars: Vec<&'a str>,
    binds: Vec<Option<String>>,
    /// The Go type of each loop variable that is not a `flickrt.Value`.
    types: Vec<Option<GoType>>,
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
    /// Whether its members run effects, independently of hidden context.
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
            let fiber = members.iter().any(|&i| {
                function(&items[i], Calls::Direct).is_some_and(|(_, params, _)| {
                    params.last().is_some_and(|p| p.stage != Stage::Pure)
                })
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

impl<'a> Gen<'a> {
    /// The Go parameters and statements of a function of `params` running
    /// `body`; with `target`, a self tail call of a pure function jumps.
    pub(super) fn function_body(
        &mut self,
        params: &[Param],
        body: &Expr,
        target: Option<Target>,
        types: &Types,
    ) -> (Vec<&'a str>, Vec<Document<'a>>) {
        let a = self.a;
        // what the body returns is of the function's Go type, and nothing
        // of a function around this one is: a function inside starts anew
        let saved = (
            self.tail.take(),
            mem::replace(&mut self.generator, false),
            mem::replace(&mut self.returns, types.result().cloned()),
            self.go_types.clone(),
        );
        let looping = target.filter(|t| {
            jumpable(params, Calls::Direct)
                && has_tail_call(body, &[(t.clone(), params.len())], Calls::Direct)
        });
        let result = match looping {
            None => {
                let (stmts, heads) = self.block(|g| {
                    let heads = g.bind_params(params);
                    for (i, p) in params.iter().enumerate() {
                        // a parameter of a Go type is that type from the start
                        if let Pat::Bind(name) = &p.pat {
                            match types.param(i) {
                                Some(gt) => {
                                    g.go_types.insert(name.clone(), gt.clone());
                                }
                                None => {
                                    g.go_types.remove(name);
                                }
                            }
                        }
                    }
                    g.tail_expr(body);
                    heads
                });
                (heads, stmts)
            }
            Some(target) => {
                let heads: Vec<&'a str> = params.iter().map(|_| self.temp()).collect();
                let vars: Vec<&'a str> = params.iter().map(|_| self.temp()).collect();
                // a loop variable of a known Go type is unboxed once, not per iteration
                let loop_types: Vec<Option<GoType>> = params
                    .iter()
                    .map(|p| {
                        matches!(p.pat, Pat::Bind(_))
                            .then(|| self.go_type_of(p.ty.as_deref()))
                            .flatten()
                    })
                    .collect();
                let tail = Tail {
                    members: vec![(target, params.len())],
                    current: 0,
                    vars: vars.clone(),
                    binds: binds(params),
                    types: loop_types.clone(),
                    state: None,
                };
                let (inner, ()) = self.block(|g| g.iteration(params, &vars, body, tail));
                let mut stmts: Vec<Document<'a>> = vars
                    .iter()
                    .zip(&heads)
                    .zip(&loop_types)
                    .enumerate()
                    .map(|(i, ((&var, &head), loop_ty))| {
                        let init = self.coerce(
                            Typed {
                                code: self.code(head),
                                ty: types.param(i).cloned(),
                            },
                            loop_ty.as_ref(),
                        );
                        let go_ty = self.go_type_name(loop_ty.as_ref());
                        statement(a, docvec![a, "var ", var, " ", go_ty, " = ", init])
                    })
                    .collect();
                // A wildcard/unit parameter can be carried only by jumps;
                // assignments do not count as Go reads of a local variable.
                stmts.extend(
                    vars.iter()
                        .map(|var| statement(a, docvec![a, "_ = ", *var])),
                );
                let body = checkpoint(a, self.fiber).into_iter().chain(inner);
                stmts.push(statement(a, docvec![a, "for ", block(a, body)]));
                (heads, stmts)
            }
        };
        (self.tail, self.generator, self.returns, self.go_types) = saved;
        result
    }

    /// One iteration of a loop: the parameters bound from the loop
    /// variables, then the body in tail position.
    fn iteration(&mut self, params: &[Param], vars: &[&'a str], body: &Expr, tail: Tail<'a>) {
        for ((p, &var), go_ty) in params.iter().zip(vars).zip(&tail.types) {
            match &p.pat {
                Pat::Bind(n) => self.bind_typed(n, self.code(var), go_ty.clone()),
                Pat::Wild | Pat::Lit(Lit::Unit) => {}
                pat => self.destructure(pat, self.a.text(var), None),
            }
        }
        self.tail = Some(tail);
        self.tail_expr(body);
        self.tail = None;
    }

    /// The parameters and body of a grouped top-level function: a call of its
    /// group.
    pub(super) fn group_call(
        &mut self,
        name: &str,
        params: &[Param],
        types: &Types,
    ) -> Option<(Vec<&'a str>, Document<'a>)> {
        let a = self.a;
        let &(g, k) = self.grouped.get(name)?;
        let arity = self.groups[g].arity;
        let group = a.alloc_str(&self.groups[g].name);
        let heads: Vec<&'a str> = params
            .iter()
            .map(|p| match &p.pat {
                Pat::Bind(n) => a.alloc_str(&names::local(n)),
                Pat::Wild | Pat::Lit(Lit::Unit) => "_",
                _ => self.temp(),
            })
            .collect();
        let mut args = vec![k.to_doc(a)];
        args.extend(heads.iter().enumerate().map(|(i, &h)| {
            if h == "_" {
                a.text("nil")
            } else if let Some(gt) = types.param(i) {
                gt.box_if_needed(a, a.text(h))
            } else {
                a.text(h)
            }
        }));
        args.resize(arity + 1, a.text("nil"));
        args.insert(0, self.context());
        // the group runs on values: its result is made the function's type
        let go = call(a, group, args);
        let go = types.result().map_or(go, |gt| gt.unbox_doc(a, go));
        Some((heads, statement(a, docvec![a, "return ", go])))
    }

    /// The Go function running the group `g` of `items`.
    pub(super) fn group(&mut self, g: usize, items: &[Item]) -> Document<'a> {
        let a = self.a;
        let (functions, members) =
            members(self.module, items, &self.groups[g].items, Calls::Direct);
        let vars: Vec<&'a str> = (0..self.groups[g].arity)
            .map(|i| a.alloc_str(&format!("v{i}")))
            .collect();
        let fiber = self.groups[g].fiber;
        self.fiber = fiber.then_some("fb");
        self.context = Some("fb");
        let mut cases = Vec::new();
        for (k, (_, params, body)) in functions.iter().enumerate() {
            // local names are unique only within a definition
            self.go_types.clear();
            self.held.clear();
            let tail = Tail {
                members: members.clone(),
                current: k,
                vars: vars.clone(),
                binds: binds(params),
                types: vec![None; vars.len()],
                state: Some("state"),
            };
            let (stmts, ()) = self.block(|g| g.iteration(params, &vars, body, tail));
            let stmts = a.concat(stmts).nest(a, INDENT);
            cases.push(statement(a, docvec![a, "case ", k, ":", stmts]));
        }
        self.go_types.clear();
        self.held.clear();
        self.fiber = None;
        self.context = None;
        let mut heads = vec!["fb *flickrt.Fiber".to_owned(), "state int".to_owned()];
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
        let (vars, binds, tail_types) = (tail.vars.clone(), tail.binds.clone(), tail.types.clone());
        let state = tail.state.filter(|_| j != tail.current);
        // a parameter passed on unchanged keeps its variable
        let changed: Vec<usize> = (0..args.len())
            .filter(|&i| !passed_on(&args, &binds, i))
            .collect();
        let changed_exprs: Vec<&Expr> = changed.iter().map(|&i| args[i]).collect();
        let wants: Vec<Option<GoType>> = changed.iter().map(|&i| tail_types[i].clone()).collect();
        let values = self.ordered_into(&changed_exprs, &wants);
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
            .is_some_and(|t| has_tail_call(rhs, &t.members, Calls::Direct))
    }

    /// `lhs && rhs` or `lhs || rhs` in tail position, its right operand a
    /// jump: the left one decides first.
    pub(super) fn tail_logic(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) {
        let a = self.a;
        let l = self.expr_bool(lhs);
        let (not, value) = if op == BinOp::And {
            ("!", "false")
        } else {
            ("", "true")
        };
        let test = docvec![a, not, l.doc];
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
