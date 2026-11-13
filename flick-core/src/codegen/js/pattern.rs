//! Pattern matching: each pattern becomes a test of the subject and the
//! bindings it makes. Arms are tried top to bottom; a guard that fails goes
//! on to the next arm.

use std::mem;

use flick_pretty::docvec;

use super::doc::{After, Before, Head, Js, MULTILINE, Stmt, block, stmt_flags, text_flags};
use super::{Gen, names};
use crate::ir::{Arm, Expr, Lit, Pat};
use flick_ir::{Constructor, JsClass, Rep};

struct Test<'d> {
    conds: Vec<Js<'d>>,
    binds: Vec<(String, String)>,
    /// Names an or-pattern assigns inside its test, declared before it.
    lets: Vec<String>,
}

impl Test<'_> {
    fn new() -> Self {
        Test {
            conds: Vec::new(),
            binds: Vec::new(),
            lets: Vec::new(),
        }
    }
}

impl<'d> Gen<'_, 'd> {
    /// A condition written out in full.
    fn cond(&self, text: &str) -> Js<'d> {
        let flags = text_flags(text);
        Js::code(self.a.text(self.a.alloc_str(text)), Head::Other, &[]).with(flags)
    }

    /// `items` joined by the operator `op`; a lone item as it is, for its
    /// place to decide on its parentheses.
    fn chain(&self, items: &[Js<'d>], op: &'d str) -> Js<'d> {
        if let [only] = items {
            return *only;
        }
        let a = self.a;
        let doc = a.join(
            items.iter().map(|j| j.at(a, Before::Other, After::Other)),
            a.text(op),
        );
        Js::code(doc, Head::Other, items)
    }

    fn test(&mut self, pat: &Pat, subject: &str, t: &mut Test<'d>) {
        let a = self.a;
        match pat {
            Pat::Wild | Pat::Lit(Lit::Unit) => {}
            Pat::Bind(name) => t.binds.push((names::ident(name), subject.to_owned())),
            Pat::Lit(lit) => {
                let lit = self.literal(lit);
                let doc = docvec![
                    a,
                    a.alloc_str(subject),
                    " === ",
                    lit.at(a, Before::Other, After::Other)
                ];
                let flags = text_flags(subject);
                t.conds.push(Js::code(doc, Head::Other, &[lit]).with(flags));
            }
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
                    .push(self.cond(&format!("{subject}.length {op} {}", at(items.len()))));
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
                if let Rep::Typeof(class) = c.rep {
                    t.conds.push(self.cond(&match class {
                        JsClass::Array => format!("Array.isArray({subject})"),
                        class => format!("typeof {subject} === {}", names::string(class.name())),
                    }));
                }
                if let Rep::Literal(l) = &c.rep {
                    t.conds
                        .push(self.cond(&format!("{subject} === {}", names::enum_literal(l))));
                }
                for arg in args {
                    self.test(arg, subject, t);
                }
            }
            Pat::Con(c, args) => {
                let (tag, fields) = self.shape(c, args.len());
                t.conds
                    .push(self.cond(&format!("{subject}._tag === {}", names::string(&tag))));
                for (arg, field) in args.iter().zip(fields) {
                    self.test(arg, &format!("{subject}.{field}"), t);
                }
            }
            Pat::Or(alternatives) => self.alternatives(alternatives, subject, t),
        }
    }

    /// The test of an or-pattern: one of the `alternatives` matches, and
    /// assigns the names it binds.
    fn alternatives(&mut self, alternatives: &[Pat], subject: &str, t: &mut Test<'d>) {
        let mut tests = Vec::new();
        for alt in alternatives {
            let mut inner = Test::new();
            self.test(alt, subject, &mut inner);
            let mut parts = inner.conds;
            if !inner.binds.is_empty() {
                let assigns: Vec<String> = inner
                    .binds
                    .iter()
                    .map(|(n, p)| format!("{n} = {p}"))
                    .collect();
                parts.push(self.cond(&format!("({}, true)", assigns.join(", "))));
            }
            for (name, _) in &inner.binds {
                if !t.lets.contains(name) {
                    t.lets.push(name.clone());
                }
            }
            t.lets.extend(inner.lets);
            tests.push(if parts.is_empty() {
                self.leaf("true")
            } else {
                self.chain(&parts, " && ")
            });
        }
        let alternatives = self.chain(&tests, " || ");
        let doc = alternatives.at(self.a, Before::Delimiter, After::Delimiter);
        t.conds
            .push(Js::code(doc, Head::Other, &[alternatives]).in_parens());
    }

    /// The `_tag` of a constructor and the fields its arguments are in.
    fn shape(&self, c: &Constructor, arity: usize) -> (String, Vec<String>) {
        match names::native(self.options.lang, c) {
            Some(names::Native::Some) => ("Some".into(), vec!["value".into()]),
            Some(names::Native::None) => ("None".into(), Vec::new()),
            Some(names::Native::Ok) => ("Success".into(), vec!["success".into()]),
            Some(names::Native::Err) => ("Failure".into(), vec!["failure".into()]),
            None => {
                let package = self.options.package(&c.module);
                let fields = (0..arity)
                    .map(|i| {
                        if i < c.arity {
                            format!("_{i}")
                        } else {
                            format!("${}", i - c.arity)
                        }
                    })
                    .collect();
                (names::tag(c, package), fields)
            }
        }
    }

    /// Binds the names of an irrefutable pattern.
    pub(super) fn destructure(&mut self, pat: &Pat, subject: &str) {
        let mut t = Test::new();
        self.test(pat, subject, &mut t);
        if !t.lets.is_empty() {
            self.buf.push(self.declare(&t.lets.join(", ")));
            let conds = self.chain(&t.conds, " && ");
            self.buf.push(self.expr_stmt(conds));
        }
        for (name, path) in t.binds {
            self.buf.push(self.constant(&name, self.leaf(&path)));
        }
    }

    /// `if (c) { body }`.
    pub(super) fn if_then(&self, c: Js<'d>, body: &[Stmt<'d>]) -> Stmt<'d> {
        let a = self.a;
        let doc = docvec![
            a,
            "if (",
            c.at(a, Before::Delimiter, After::Delimiter),
            ") ",
            block(a, body)
        ];
        Stmt::new(doc, c.flags | stmt_flags(body) | MULTILINE)
    }

    /// `head { body }`, a statement of the lines of `body`.
    fn braced(&self, head: String, body: &[Stmt<'d>]) -> Stmt<'d> {
        let flags = text_flags(&head) | stmt_flags(body) | MULTILINE;
        Stmt::new(docvec![self.a, head, block(self.a, body)], flags)
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
                g.buf.push(g.assign(r, v));
                g.buf.push(g.line(&format!("break {label};")));
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
                let head = case.map_or("default: ".to_owned(), |tag| format!("case {tag}: "));
                body.push(self.braced(head, &stmts));
            }
            let by_value = arms
                .iter()
                .any(|a| matches!(&a.pat, Pat::Con(c, _) if matches!(c.rep, Rep::Literal(_))));
            let on = if by_value {
                subject.to_owned()
            } else {
                format!("{subject}._tag")
            };
            out.push(self.braced(format!("switch ({on}) "), &body));
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
            Some(label) => self.buf.push(self.braced(format!("{label}: "), &out)),
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
                    g.buf.push(Stmt::ret(g.a, None, handler));
                },
            );
        }
        let undefined = self.leaf("undefined");
        self.buf.push(Stmt::ret(self.a, None, undefined));
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
                Pat::Con(c, args) if matches!(c.rep, Rep::Literal(_)) && args.is_empty() => {
                    let Rep::Literal(l) = &c.rep else {
                        unreachable!()
                    };
                    Some(names::enum_literal(l))
                }
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
        let mut t = Test::new();
        self.test(pat, subject, &mut t);
        if exhaustive && guard.is_none() && t.lets.is_empty() {
            t.conds.clear();
        }
        let (inner, ()) = self.block(|g| {
            for (name, path) in &t.binds {
                let c = g.constant(name, g.leaf(path));
                g.buf.push(c);
            }
            match guard {
                Some(guard) => {
                    let saved = mem::replace(&mut g.generator, false);
                    let c = g.expr(guard);
                    g.generator = saved;
                    let (b, ()) = g.block(|g| body(g, value));
                    let stmt = g.if_then(c, &b);
                    g.buf.push(stmt);
                }
                None => body(g, value),
            }
        });
        let mut block = Vec::new();
        if t.conds.is_empty() {
            block.extend(inner);
        } else {
            let conds = self.chain(&t.conds, " && ");
            block.push(self.if_then(conds, &inner));
        }
        // names are unique in a definition: only the or-pattern's need a block
        if t.lets.is_empty() {
            self.buf.extend(block);
        } else {
            block.insert(0, self.declare(&t.lets.join(", ")));
            self.buf.push(self.braced(String::new(), &block));
        }
    }
}
