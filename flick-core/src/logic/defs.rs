//! The definitions a statement names, admitted into the domain. Each body is
//! read strictly from the IR its module is compiled to, a method's from the
//! instance its dictionary resolves to, and the closure of what they name has
//! no cycle. Recursion, foreign code, a context or anything else the domain
//! refuses keeps a definition, and every statement naming it, out.

use std::iter::once;
use std::rc::Rc;

use flick_ir::{Dict, Expr, InstanceInfo, Item, ItemKind};

use super::term::{Def, Refused, Term, term};
use crate::source::Span;

/// The compiled modules a proof reads definitions from.
pub trait Program {
    fn item(&self, module: &str, name: &str) -> Option<&Item>;
    fn instance(&self, module: &str, name: &str) -> Option<(&InstanceInfo, &[(String, Expr)])>;
}

/// The admitted definitions with their bodies, in the order admitted: a
/// body names only definitions before it.
#[derive(Default)]
pub struct Admitted(Vec<(Rc<Def>, Term)>);

impl Admitted {
    /// The definitions with their bodies, each naming only those before it.
    pub fn defs(&self) -> &[(Rc<Def>, Term)] {
        &self.0
    }

    pub fn body(&self, d: &Def) -> Option<&Term> {
        self.0.iter().find(|(k, _)| **k == *d).map(|(_, t)| t)
    }
}

fn refuse<T>(span: Span, why: String) -> Result<T, Refused> {
    Err(Refused { span, why })
}

/// Admits every definition `terms` name, and what those name in turn.
pub fn admit(terms: &[&Term], program: &dyn Program, span: Span) -> Result<Admitted, Refused> {
    let mut admitting = Admitting {
        program,
        span,
        out: Admitted::default(),
        path: Vec::new(),
    };
    for t in terms {
        admitting.visit(t)?;
    }
    Ok(admitting.out)
}

struct Admitting<'a> {
    program: &'a dyn Program,
    span: Span,
    out: Admitted,
    /// The definitions being admitted, outermost first.
    path: Vec<Rc<Def>>,
}

impl Admitting<'_> {
    fn visit(&mut self, t: &Term) -> Result<(), Refused> {
        match t {
            Term::Var(_) | Term::Lit(_) | Term::Con(_) => Ok(()),
            Term::Lam(_, b) => self.visit(b),
            Term::App(f, a) | Term::Let(_, f, a) => {
                self.visit(f)?;
                self.visit(a)
            }
            Term::Match(s, arms) => {
                self.visit(s)?;
                arms.iter().try_for_each(|a| self.visit(&a.body))
            }
            Term::Def(d) => self.definition(d),
        }
    }

    fn definition(&mut self, d: &Rc<Def>) -> Result<(), Refused> {
        if self.out.body(d).is_some() {
            return Ok(());
        }
        if let Some(at) = self.path.iter().position(|p| p == d) {
            let cycle: Vec<String> = self.path[at..]
                .iter()
                .map(|p| format!("`{}`", p.shown()))
                .collect();
            return refuse(
                self.span,
                format!(
                    "{} names itself: recursion is outside the domain",
                    cycle.join(" → ")
                ),
            );
        }
        let body = self.body_of(d).map_err(|mut failure| {
            if !self.path.is_empty() {
                let chain: Vec<_> = self
                    .path
                    .iter()
                    .chain(once(d))
                    .map(|def| format!("`{}`", def.shown()))
                    .collect();
                failure.why.push_str("; dependency chain ");
                failure.why.push_str(&chain.join(" → "));
            }
            failure
        })?;
        self.path.push(d.clone());
        self.visit(&body)?;
        self.path.pop();
        self.out.0.push((d.clone(), body));
        Ok(())
    }

    fn body_of(&self, d: &Def) -> Result<Term, Refused> {
        let shown = d.shown();
        let item = match d {
            Def::Value { module, name } => self.program.item(module, name).map(|i| &i.kind),
            Def::Method { dict, name } => {
                let (module, instance) = self.resolve(dict)?;
                let Some((info, methods)) = self.program.instance(&module, &instance) else {
                    return refuse(
                        self.span,
                        format!("the instance `{instance}` is not available to this proof"),
                    );
                };
                if !info.context.is_empty() {
                    return refuse(
                        self.span,
                        format!("`{shown}` of `{instance}` takes dictionaries of its context"),
                    );
                }
                if info.defaults.iter().any(|(m, ..)| m == name) {
                    return refuse(
                        self.span,
                        format!("`{shown}` of `{instance}` is a default method, not admitted yet"),
                    );
                }
                return match methods.iter().find(|(m, _)| m == name) {
                    Some((_, body)) => term(body),
                    None => refuse(self.span, format!("`{instance}` has no method `{shown}`")),
                };
            }
        };
        match item {
            Some(ItemKind::Value { body, dicts, .. }) if dicts.is_empty() => term(body),
            Some(ItemKind::Value { .. }) => refuse(
                self.span,
                format!("`{shown}` takes dictionaries, which a proof does not pass yet"),
            ),
            Some(ItemKind::Foreign(_)) => refuse(
                self.span,
                format!("`{shown}` is foreign code, which the domain does not admit"),
            ),
            Some(_) => refuse(
                self.span,
                format!("`{shown}` is no definition a proof unfolds"),
            ),
            None => refuse(
                self.span,
                format!("the definition of `{shown}` is not available to this proof"),
            ),
        }
    }

    /// The instance a dictionary is: by its head, or a superclass field of one.
    fn resolve(&self, dict: &Dict) -> Result<(String, String), Refused> {
        match dict {
            Dict::Instance { module, name, args } if args.is_empty() => {
                Ok((module.clone(), name.clone()))
            }
            Dict::Super { dict, class } => {
                let (module, name) = self.resolve(dict)?;
                let supers = self
                    .program
                    .instance(&module, &name)
                    .map(|(info, _)| &info.supers);
                // two superclasses may share a field's name: then it is neither
                let mut found = supers.into_iter().flatten().filter(|(c, _)| c == class);
                match (found.next(), found.next()) {
                    (Some((_, inner)), None) => self.resolve(inner),
                    _ => refuse(
                        self.span,
                        format!("the `{class}` of `{name}` is not available to this proof"),
                    ),
                }
            }
            _ => refuse(
                self.span,
                "a dictionary that is no instance without a context is not followed yet".into(),
            ),
        }
    }
}
