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
                .push(format!("{subject} === {}", super::literal::literal(lit))),
            Pat::Tuple(items) => {
                for (i, item) in items.iter().enumerate() {
                    self.test(item, &format!("{subject}[{i}]"), t);
                }
            }
            Pat::List(items, rest) => {
                // a walked list starts at its offset (see `view`)
                let view = self.views.get(subject).cloned();
                let at = |n: usize| match &view {
                    Some((offset, _)) if n == 0 => offset.clone(),
                    Some((offset, _)) => format!("{offset} + {n}"),
                    None => n.to_string(),
                };
                let op = if rest.is_some() { ">=" } else { "===" };
                t.conds
                    .push(format!("{subject}.length {op} {}", at(items.len())));
                for (i, item) in items.iter().enumerate() {
                    self.test(item, &format!("{subject}[{}]", at(i)), t);
                }
                match (rest.as_deref(), &view) {
                    // a jump moves the offset there instead
                    (Some(Pat::Bind(r)), Some((_, pos))) => {
                        self.rests.insert(r.clone(), (*pos, at(items.len())));
                    }
                    (Some(rest), _) => {
                        self.test(rest, &format!("{subject}.slice({})", at(items.len())), t);
                    }
                    (None, _) => {}
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
                match &c.rep {
                    Rep::Typeof(JsClass::Array) => {
                        t.conds.push(format!("Array.isArray({subject})"));
                    }
                    Rep::Typeof(class) => t.conds.push(format!(
                        "typeof {subject} === {}",
                        names::string(class.name())
                    )),
                    Rep::Literal(l) => t
                        .conds
                        .push(format!("{subject} === {}", names::enum_literal(l))),
                    Rep::Tagged | Rep::Unboxed => {}
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
                // the fields, then the dictionaries a value of a GADT holds
                let fields = (0..arity)
                    .map(|i| if i < c.arity { format!("_{i}") } else { format!("${}", i - c.arity) })
                    .collect();
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
        let emit = |g: &mut Self, body: &Expr| match (result, &label) {
            (Some(r), Some(label)) => {
                let v = g.expr(body);
                g.buf.push(format!("{r} = {v};"));
                g.buf.push(format!("break {label};"));
            }
            _ => g.tail_expr(body),
        };
        let release = self.options.release;
        let mut out = Vec::new();
        if release && let Some(cases) = self.switch_cases(arms) {
            let mut body = Vec::new();
            for (case, arm) in cases.into_iter().zip(arms) {
                let (stmts, ()) = self
                    .block(|g| g.arm(subject, &arm.pat, arm.guard.as_ref(), &arm.body, true, emit));
                let head = case.map_or("default:".to_owned(), |tag| format!("case {tag}:"));
                body.push(format!("{head} {{\n{}}}", indent(&stmts)));
            }
            // an enum's constructors are their literals: the switch is on the value
            let by_value = arms
                .iter()
                .any(|a| matches!(&a.pat, Pat::Con(c, _) if matches!(c.rep, Rep::Literal(_))));
            let on = if by_value {
                subject.to_owned()
            } else {
                format!("{subject}._tag")
            };
            out.push(format!("switch ({on}) {{\n{}}}", indent(&body)));
        } else {
            for (i, arm) in arms.iter().enumerate() {
                // the match is exhaustive: its last arm without a guard needs no test
                let last = release && i + 1 == arms.len();
                let (stmts, ()) = self
                    .block(|g| g.arm(subject, &arm.pat, arm.guard.as_ref(), &arm.body, last, emit));
                out.extend(stmts);
            }
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
            self.arm(
                subject,
                &arm.pat,
                arm.guard.as_ref(),
                &arm.body,
                false,
                |g, body| {
                    let handler = g.gen_block(body);
                    g.buf.push(format!("return {handler};"));
                },
            );
        }
        self.buf.push("return undefined;".to_owned());
    }

    /// The `case` tag of each arm of a `match` a `switch` can take, `None`
    /// for the last arm, which is `default`: three or more arms without
    /// guards, each a tagged constructor of distinct tag whose fields are
    /// only bound, or for the last arm any binding.
    fn switch_cases(&self, arms: &[Arm]) -> Option<Vec<Option<String>>> {
        if arms.len() < 3 || arms.iter().any(|a| a.guard.is_some()) {
            return None;
        }
        let plain = |p: &Pat| matches!(p, Pat::Wild | Pat::Bind(_) | Pat::Lit(Lit::Unit));
        let mut cases = Vec::new();
        for (i, arm) in arms.iter().enumerate() {
            let case = match &arm.pat {
                Pat::Con(c, args) if c.rep == Rep::Tagged && args.iter().all(plain) => {
                    Some(names::string(&self.shape(c, args.len()).0))
                }
                Pat::Con(c, _) if let Rep::Literal(l) = &c.rep => Some(names::enum_literal(l)),
                p if plain(p) && i + 1 == arms.len() => None,
                _ => return None,
            };
            if case.is_some() && cases.contains(&case) {
                return None;
            }
            cases.push(case);
        }
        if let Some(last) = cases.last_mut() {
            *last = None;
        }
        Some(cases)
    }

    /// An arm on `subject`; `exhaustive`, it matches whatever reaches it.
    /// Runs `body` on `value` when `pat` matches `subject` and `guard`
    /// holds, with the names the pattern binds.
    pub(super) fn arm(
        &mut self,
        subject: &str,
        pat: &Pat,
        guard: Option<&Expr>,
        value: &Expr,
        exhaustive: bool,
        body: impl FnOnce(&mut Self, &Expr),
    ) {
        let mut t = Test::default();
        self.test(pat, subject, &mut t);
        if exhaustive && guard.is_none() {
            t.conds.clear();
        }
        let (inner, ()) = self.block(|g| {
            for (name, path) in &t.binds {
                let c = g.constant(name, path);
                g.buf.push(c);
            }
            match guard {
                Some(guard) => {
                    let saved = mem::replace(&mut g.generator, false);
                    let c = g.expr(guard);
                    g.generator = saved;
                    let (b, ()) = g.block(|g| body(g, value));
                    g.buf.push(format!("if ({c}) {{\n{}}}", indent(&b)));
                }
                None => body(g, value),
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
