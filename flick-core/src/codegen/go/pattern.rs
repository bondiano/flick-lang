//! Pattern matching: each pattern becomes a test of the subject and the
//! bindings it makes. Arms are tried top to bottom; a guard that fails goes
//! on to the next arm.

use super::expr::literal;
use super::{Gen, indent, names};
use crate::ir::{Arm, Expr, Lit, Pat};
use crate::types::Rep;

#[derive(Default)]
struct Test {
    conds: Vec<String>,
    binds: Vec<(String, String)>,
    /// Locals an or-pattern assigns inside its test, declared before it.
    lets: Vec<String>,
}

impl Gen<'_> {
    fn test(&mut self, pat: &Pat, subject: &str, t: &mut Test) {
        match pat {
            Pat::Wild | Pat::Lit(Lit::Unit) => {}
            Pat::Bind(name) => t.binds.push((names::local(name), subject.to_owned())),
            Pat::Lit(lit) => t
                .conds
                .push(format!("flickrt.Same({subject}, {})", literal(lit))),
            Pat::Tuple(items) => {
                for (i, item) in items.iter().enumerate() {
                    self.test(item, &format!("flickrt.At({subject}, {i})"), t);
                }
            }
            Pat::List(items, rest) => {
                let op = if rest.is_some() { ">=" } else { "==" };
                t.conds
                    .push(format!("flickrt.Len({subject}) {op} {}", items.len()));
                for (i, item) in items.iter().enumerate() {
                    self.test(item, &format!("flickrt.At({subject}, {i})"), t);
                }
                if let Some(rest) = rest {
                    self.test(
                        rest,
                        &format!("flickrt.Rest({subject}, {})", items.len()),
                        t,
                    );
                }
            }
            Pat::Record(fields) => {
                for (label, p) in fields {
                    self.test(
                        p,
                        &format!("flickrt.Field({subject}, {})", names::string(label)),
                        t,
                    );
                }
            }
            Pat::Con(c, args) if c.rep != Rep::Tagged => {
                if let Rep::Typeof(_) = c.rep {
                    self.errors.push((
                        "an `untagged` type is not available on the go target yet".to_owned(),
                        None,
                    ));
                }
                for arg in args {
                    self.test(arg, subject, t);
                }
            }
            Pat::Con(c, args) => {
                let tag = names::string(&self.tag(c));
                t.conds.push(format!("flickrt.IsTag({subject}, {tag})"));
                for (i, arg) in args.iter().enumerate() {
                    self.test(arg, &format!("flickrt.Get({subject}, {i})"), t);
                }
            }
            Pat::Or(alternatives) => {
                let mut tests = Vec::new();
                for alt in alternatives {
                    let mut inner = Test::default();
                    self.test(alt, subject, &mut inner);
                    let cond = if inner.conds.is_empty() {
                        "true".to_owned()
                    } else {
                        inner.conds.join(" && ")
                    };
                    let assigns = inner
                        .binds
                        .iter()
                        .fold(String::new(), |acc, (n, p)| acc + n + " = " + p + "; ");
                    tests.push(format!("if {cond} {{ {assigns}return true }}"));
                    for (name, _) in &inner.binds {
                        if !t.lets.contains(name) {
                            t.lets.push(name.clone());
                        }
                    }
                    t.lets.extend(inner.lets);
                }
                t.conds.push(format!(
                    "func() bool {{ {}; return false }}()",
                    tests.join("; ")
                ));
            }
        }
    }

    fn declare_lets(&mut self, lets: &[String]) {
        for l in lets {
            self.buf.push(format!("var {l} flickrt.Value\n_ = {l}"));
        }
    }

    /// Binds the names of an irrefutable pattern.
    pub(super) fn destructure(&mut self, pat: &Pat, subject: &str) {
        let mut t = Test::default();
        self.test(pat, subject, &mut t);
        if !t.lets.is_empty() {
            self.declare_lets(&t.lets);
            self.buf.push(format!("_ = {}", t.conds.join(" && ")));
        }
        for (name, path) in t.binds {
            self.buf
                .push(format!("var {name} flickrt.Value = {path}\n_ = {name}"));
        }
    }

    /// The arms of a `match` on `subject`: with `result`, the value goes
    /// there; otherwise each arm is in tail position and returns.
    pub(super) fn arms(&mut self, subject: &str, arms: &[Arm], result: Option<&str>) {
        let mut out = Vec::new();
        for arm in arms {
            let (stmts, ()) = self.block(|g| {
                g.arm(
                    subject,
                    &arm.pat,
                    arm.guard.as_ref(),
                    &arm.body,
                    |g, body| match result {
                        Some(r) => {
                            let v = g.expr(body);
                            g.buf.push(format!("{r} = {v}"));
                            g.buf.push("break".to_owned());
                        }
                        None => g.tail_expr(body),
                    },
                );
            });
            out.extend(stmts);
        }
        if result.is_some() {
            // a loop run once, so that an arm can leave it with `break`
            out.push("flickrt.NoMatch()".to_owned());
            self.buf.push(format!("for {{\n{}}}", indent(&out)));
        } else {
            self.buf.extend(out);
            self.buf.push("return flickrt.NoMatch()".to_owned());
        }
    }

    /// Runs `body` on `value` when `pat` matches `subject` and `guard`
    /// holds, with the names the pattern binds.
    pub(super) fn arm(
        &mut self,
        subject: &str,
        pat: &Pat,
        guard: Option<&Expr>,
        value: &Expr,
        body: impl FnOnce(&mut Self, &Expr),
    ) {
        let mut t = Test::default();
        self.test(pat, subject, &mut t);
        let (inner, ()) = self.block(|g| {
            for (name, path) in &t.binds {
                g.buf
                    .push(format!("var {name} flickrt.Value = {path}\n_ = {name}"));
            }
            match guard {
                Some(guard) => {
                    let c = g.expr(guard);
                    let (b, ()) = g.block(|g| body(g, value));
                    g.buf
                        .push(format!("if flickrt.Bool({c}) {{\n{}}}", indent(&b)));
                }
                None => body(g, value),
            }
        });
        let mut block = Vec::new();
        if t.conds.is_empty() {
            block.push(format!("{{\n{}}}", indent(&inner)));
        } else {
            block.push(format!(
                "if {} {{\n{}}}",
                t.conds.join(" && "),
                indent(&inner)
            ));
        }
        if t.lets.is_empty() {
            self.buf.extend(block);
        } else {
            let (mut lets, ()) = self.block(|g| g.declare_lets(&t.lets));
            lets.extend(block);
            self.buf.push(format!("{{\n{}}}", indent(&lets)));
        }
    }
}
