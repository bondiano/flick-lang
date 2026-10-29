//! Functions and calls: a known top-level function applied to all its
//! arguments is a direct Go call; any other application goes through the
//! curried value.

use std::slice;

use super::{Gen, Shape, indent, names, param_list};
use crate::ir::{Expr, ExprKind, Lit, Param, Pat};
use crate::types::{Dict, Rep};

/// How to build a call from its Go arguments.
type Call = Box<dyn Fn(&[String]) -> String>;

impl Gen<'_> {
    /// A constrained value given its dictionaries.
    pub(super) fn given(&mut self, inner: &Expr, dicts: &[Dict]) -> String {
        let dicts: Vec<String> = dicts
            .iter()
            .map(|d| {
                let d = self.dict(d);
                self.spill(d)
            })
            .collect();
        if let ExprKind::Global { module, name } = &inner.kind {
            return self.global_value(module, name, dicts);
        }
        let f = self.expr(inner);
        format!("flickrt.Inst({f}, {})", dicts.join(", "))
    }

    /// A value that takes the dictionaries `shown` before it is computed.
    pub(super) fn abstraction(&mut self, shown: &[String], inner: &Expr) -> String {
        let mark = self.givens.len();
        let dicts = self.bind_givens(shown);
        let (mut stmts, v) = self.block(|g| g.expr(inner));
        self.givens.truncate(mark);
        let takes: Vec<String> = dicts
            .iter()
            .enumerate()
            .map(|(i, d)| format!("{d} := ds[{i}]\n_ = {d}"))
            .collect();
        stmts.splice(0..0, takes);
        stmts.push(format!("return {v}"));
        format!(
            "flickrt.Poly(func(ds ...flickrt.Value) flickrt.Value {{\n{}}})",
            indent(&stmts)
        )
    }

    /// A top-level value of `module` with `dicts` given: a function of
    /// parameters becomes a curried value.
    pub(super) fn global_value(&mut self, module: &str, name: &str, dicts: Vec<String>) -> String {
        self.reads(module);
        match self.shape_of(module, name) {
            Shape::Var => names::var(module, name),
            Shape::Func { dicts: n, params } if dicts.len() == n => {
                let f = names::func(module, name);
                match params {
                    0 => format!("{f}({})", dicts.join(", ")),
                    1 if n == 0 => f,
                    _ => self.curried(&f, dicts, params),
                }
            }
            // a constrained value passed on before its dictionaries
            Shape::Func { dicts: n, params } => {
                let given: Vec<String> = (0..n).map(|i| format!("ds[{i}]")).collect();
                let f = names::func(module, name);
                let inner = if params == 0 {
                    format!("{f}({})", given.join(", "))
                } else {
                    self.curried(&f, given, params)
                };
                format!(
                    "flickrt.Poly(func(ds ...flickrt.Value) flickrt.Value {{ return {inner} }})"
                )
            }
        }
    }

    /// The curried function that calls `f` with `given`, then `n` more
    /// arguments taken one at a time.
    fn curried(&mut self, f: &str, given: Vec<String>, n: usize) -> String {
        let heads: Vec<String> = (0..n).map(|_| self.temp()).collect();
        let args: Vec<String> = given.into_iter().chain(heads.iter().cloned()).collect();
        let mut body = format!("{f}({})", args.join(", "));
        for h in heads.iter().rev() {
            body = format!("flickrt.Fn(func({h} flickrt.Value) flickrt.Value {{ return {body} }})");
        }
        body
    }

    /// An application, or a constructor: a known function of all its
    /// arguments is called directly.
    pub(super) fn app(&mut self, e: &Expr) -> String {
        let mut head = e;
        let mut first = None;
        if let ExprKind::App {
            func,
            arg,
            arg_first: true,
            ..
        } = &e.kind
        {
            let a = self.expr(arg);
            first = Some(self.spill(a));
            head = func;
        }
        let mut args = Vec::new();
        while let ExprKind::App {
            func,
            arg,
            arg_first: false,
            ..
        } = &head.kind
        {
            args.push(&**arg);
            head = func;
        }
        args.reverse();
        let (call, given, arity): (Call, Vec<String>, usize) = match &head.kind {
            ExprKind::Con(c) => {
                let tag = names::string(&self.tag(c));
                let call: Call = match c.rep {
                    Rep::Tagged if c.arity == 0 => {
                        Box::new(move |_| format!("flickrt.NewData({tag})"))
                    }
                    Rep::Tagged => Box::new(move |a: &[String]| {
                        format!("flickrt.NewData({tag}, {})", a.join(", "))
                    }),
                    Rep::Unboxed => Box::new(|a: &[String]| a[0].clone()),
                    Rep::Typeof(_) => {
                        self.unsupported(head.span, "an `untagged` type");
                        Box::new(|a: &[String]| a[0].clone())
                    }
                };
                (call, Vec::new(), c.arity)
            }
            ExprKind::Global { module, name } => self.direct(module, name, Vec::new()),
            ExprKind::Dicts(inner, dicts) if matches!(inner.kind, ExprKind::Global { .. }) => {
                let ExprKind::Global { module, name } = &inner.kind else {
                    unreachable!("a global, as the guard tested");
                };
                let dicts: Vec<String> = dicts.iter().map(|d| self.dict(d)).collect();
                self.direct(module, name, dicts)
            }
            _ => {
                let f = self.expr(head);
                let f = self.spill(f);
                (Box::new(move |_| f.clone()), Vec::new(), 0)
            }
        };
        let mut values = self.ordered(&args);
        values.extend(first);
        if values.len() < arity {
            // a partial application keeps what it was given
            let missing = arity - values.len();
            let given: Vec<String> = given
                .into_iter()
                .chain(values)
                .map(|v| self.spill(v))
                .collect();
            let heads: Vec<String> = (0..missing).map(|_| self.temp()).collect();
            let all: Vec<String> = given.into_iter().chain(heads.iter().cloned()).collect();
            let mut body = call(&all);
            for h in heads.iter().rev() {
                body = format!(
                    "flickrt.Fn(func({h} flickrt.Value) flickrt.Value {{ return {body} }})"
                );
            }
            return body;
        }
        let rest = values.split_off(arity);
        let all: Vec<String> = given.into_iter().chain(values).collect();
        let mut v = call(&all);
        for a in rest {
            v = format!("flickrt.Apply({v}, {a})");
        }
        v
    }

    /// How to call a top-level value of `module`: its Go function and how
    /// many parameters it takes after `dicts`, or the value to apply.
    fn direct(
        &mut self,
        module: &str,
        name: &str,
        dicts: Vec<String>,
    ) -> (Call, Vec<String>, usize) {
        self.reads(module);
        match self.shape_of(module, name) {
            Shape::Func { dicts: n, params } if n == dicts.len() && params > 0 => {
                let f = names::func(module, name);
                (
                    Box::new(move |a: &[String]| format!("{f}({})", a.join(", "))),
                    dicts,
                    params,
                )
            }
            _ => {
                let f = self.global_value(module, name, dicts);
                (Box::new(move |_| f.clone()), Vec::new(), 0)
            }
        }
    }

    /// Binds the parameters of a function to Go parameter names; a pattern
    /// is destructured into the current statements.
    pub(super) fn bind_params(&mut self, params: &[Param]) -> Vec<String> {
        let mut heads = Vec::new();
        for p in params {
            let name = match &p.pat {
                Pat::Bind(n) => names::local(n),
                Pat::Wild | Pat::Lit(Lit::Unit) => "_".to_owned(),
                pat => {
                    let t = self.temp();
                    self.destructure(pat, &t);
                    t
                }
            };
            heads.push(name);
        }
        heads
    }

    /// A curried function: one Go function per stage, the body at the last.
    pub(super) fn lambda(&mut self, params: &[Param], body: &Expr) -> String {
        let (stmts, heads) = self.block(|g| {
            let heads = g.bind_params(params);
            g.tail_expr(body);
            heads
        });
        let mut text = format!(
            "func({}) flickrt.Value {{\n{}}}",
            param_list(&heads[heads.len() - 1..]),
            indent(&stmts)
        );
        for h in heads[..heads.len() - 1].iter().rev() {
            text = format!(
                "func({}) flickrt.Value {{\n\treturn flickrt.Fn({})\n}}",
                param_list(slice::from_ref(h)),
                text.replace('\n', "\n\t")
            );
        }
        format!("flickrt.Fn({text})")
    }
}
