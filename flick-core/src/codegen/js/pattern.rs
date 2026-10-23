//! Pattern matching: each pattern becomes a test of the subject and the
//! bindings it makes. Arms are tried top to bottom; a guard that fails goes
//! on to the next arm.

use super::{Gen, indent, names};
use std::mem;

use crate::ir::{Arm, Expr, Lit, Pat};
use crate::types::{Constructor, JsClass, Rep};

#[derive(Default)]
struct Test {
    conds: Vec<String>,
    binds: Vec<(String, String)>,
    /// Names an or-pattern assigns inside its test, declared before it.
    lets: Vec<String>,
}

impl Gen<'_> {
    fn test(&mut self, pat: &Pat, subject: &str, t: &mut Test) {
        match pat {
            Pat::Wild | Pat::Lit(Lit::Unit) => {}
            Pat::Bind(name) => t.binds.push((names::ident(name), subject.to_owned())),
            Pat::Lit(lit) => t
                .conds
                .push(format!("{subject} === {}", super::expr::literal(lit))),
            Pat::Tuple(items) => {
                for (i, item) in items.iter().enumerate() {
                    self.test(item, &format!("{subject}[{i}]"), t);
                }
            }
            Pat::List(items) => {
                t.conds
                    .push(format!("{subject}.length === {}", items.len()));
                for (i, item) in items.iter().enumerate() {
                    self.test(item, &format!("{subject}[{i}]"), t);
                }
            }
            Pat::Record(fields) => {
                for (label, p) in fields {
                    let path = if names::is_property(label) {
                        format!("{subject}.{label}")
                    } else {
                        format!("{subject}[{}]", names::string(label))
                    };
                    self.test(p, &path, t);
                }
            }
            Pat::Con(c, args) if c.rep != Rep::Tagged => {
                if let Rep::Typeof(class) = c.rep {
                    t.conds.push(match class {
                        JsClass::Array => format!("Array.isArray({subject})"),
                        class => format!("typeof {subject} === {}", names::string(class.name())),
                    });
                }
                for arg in args {
                    self.test(arg, subject, t);
                }
            }
            Pat::Con(c, args) => {
                let (tag, fields) = self.shape(c, args.len());
                t.conds
                    .push(format!("{subject}._tag === {}", names::string(&tag)));
                for (arg, field) in args.iter().zip(fields) {
                    self.test(arg, &format!("{subject}.{field}"), t);
                }
            }
            Pat::Or(alternatives) => {
                let mut tests = Vec::new();
                for alt in alternatives {
                    let mut inner = Test::default();
                    self.test(alt, subject, &mut inner);
                    let mut parts = inner.conds;
                    if !inner.binds.is_empty() {
                        let assigns: Vec<String> = inner
                            .binds
                            .iter()
                            .map(|(n, p)| format!("({n} = {p})"))
                            .collect();
                        parts.push(format!("({}, true)", assigns.join(", ")));
                    }
                    for (name, _) in &inner.binds {
                        if !t.lets.contains(name) {
                            t.lets.push(name.clone());
                        }
                    }
                    t.lets.extend(inner.lets);
                    tests.push(if parts.is_empty() {
                        "true".to_owned()
                    } else {
                        parts.join(" && ")
                    });
                }
                t.conds.push(format!("({})", tests.join(" || ")));
            }
        }
    }

    /// The `_tag` of a constructor and the fields its arguments are in.
    fn shape(&self, c: &Constructor, arity: usize) -> (String, Vec<String>) {
        match names::native(c) {
            Some(names::Native::Some) => ("Some".into(), vec!["value".into()]),
            Some(names::Native::None) => ("None".into(), Vec::new()),
            Some(names::Native::Ok) => ("Success".into(), vec!["success".into()]),
            Some(names::Native::Err) => ("Failure".into(), vec!["failure".into()]),
            None => {
                let package = (self.options.package)(&c.module);
                let fields = (0..arity).map(|i| format!("_{i}")).collect();
                (names::tag(c, package.as_deref()), fields)
            }
        }
    }

    /// Binds the names of an irrefutable pattern.
    pub(super) fn destructure(&mut self, pat: &Pat, subject: &str) {
        let mut t = Test::default();
        self.test(pat, subject, &mut t);
        if !t.lets.is_empty() {
            self.buf.push(self.declare(&t.lets.join(", ")));
            self.buf.push(format!("{};", t.conds.join(" && ")));
        }
        for (name, path) in t.binds {
            self.buf.push(self.constant(&name, &path));
        }
    }

    /// The arms of a `match` on `subject`: with `result`, the value goes
    /// there; otherwise each arm is in tail position.
    pub(super) fn arms(&mut self, subject: &str, arms: &[Arm], result: Option<&str>) {
        let label = result.map(|_| format!("$m{}", self.temps + 1));
        if label.is_some() {
            self.temps += 1;
        }
        let mut out = Vec::new();
        for arm in arms {
            let (stmts, ()) = self.block(|g| {
                g.arm(subject, arm, |g, body| match (result, &label) {
                    (Some(r), Some(label)) => {
                        let v = g.expr(body);
                        g.buf.push(format!("{r} = {v};"));
                        g.buf.push(format!("break {label};"));
                    }
                    _ => g.tail_expr(body),
                });
            });
            out.extend(stmts);
        }
        match label {
            Some(label) => self.buf.push(format!("{label}: {{\n{}}}", indent(&out))),
            None => self.buf.extend(out),
        }
    }

    /// The arms of `try … with` on the error `subject`: a matching arm
    /// returns its handler, and no match returns `undefined`.
    pub(super) fn handlers(&mut self, subject: &str, arms: &[Arm]) {
        for arm in arms {
            self.arm(subject, arm, |g, body| {
                let handler = g.gen_block(body);
                g.buf.push(format!("return {handler};"));
            });
        }
        self.buf.push("return undefined;".to_owned());
    }

    fn arm(&mut self, subject: &str, arm: &Arm, body: impl FnOnce(&mut Self, &Expr)) {
        let mut t = Test::default();
        self.test(&arm.pat, subject, &mut t);
        let (inner, ()) = self.block(|g| {
            for (name, path) in &t.binds {
                let c = g.constant(name, path);
                g.buf.push(c);
            }
            match &arm.guard {
                Some(guard) => {
                    let saved = mem::replace(&mut g.generator, false);
                    let c = g.expr(guard);
                    g.generator = saved;
                    let (b, ()) = g.block(|g| body(g, &arm.body));
                    g.buf.push(format!("if ({c}) {{\n{}}}", indent(&b)));
                }
                None => body(g, &arm.body),
            }
        });
        let mut block = Vec::new();
        if t.conds.is_empty() {
            block.extend(inner);
        } else {
            block.push(format!(
                "if ({}) {{\n{}}}",
                t.conds.join(" && "),
                indent(&inner)
            ));
        }
        // names are unique in a definition: only the or-pattern's need a block
        if t.lets.is_empty() {
            self.buf.extend(block);
        } else {
            block.insert(0, self.declare(&t.lets.join(", ")));
            self.buf.push(format!("{{\n{}}}", indent(&block)));
        }
    }
}
