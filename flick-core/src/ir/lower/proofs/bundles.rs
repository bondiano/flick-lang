//! A bundle checked: each law of its class and of each superclass, at the
//! bundle's dictionary and types, is proved by the lemma its record names
//! for it. The lemma is applied to the law's parameters, so it must be
//! about exactly the definitions the dictionary has: a lemma of another
//! instance, or of a local dictionary, proves nothing of it.

use std::rc::Rc;

use flick_ir::{Dict, Expr, ExprKind};

use super::{Field, Proven, statement};
use crate::diagnostics::Diagnostic;
use crate::logic::{self, Arg, Cert, Failure, Rule, Term, Ty};
use crate::source::Span;
use crate::types::{Bundle, GoalType};

/// The laws of a class, by its module and name; `None` where its module's
/// laws are not available to the run.
pub type Laws<'a, 'p> = &'a dyn Fn(&str, &str) -> Option<Vec<&'p Proven>>;

fn invalid(span: Span, why: String) -> Failure {
    Failure::Invalid { span, why }
}

/// What fails of the bundle `lowered` states, as `bundle` types it.
pub(super) fn check_bundle(
    (lowered, info): &Proven,
    bundle: &Bundle,
    laws: Laws<'_, '_>,
    program: &dyn logic::Program,
    lemmas: &[logic::Lemma],
) -> Vec<Diagnostic> {
    let Some(subject) = &lowered.subject else {
        let why = "its constraint resolved to no dictionary".into();
        return vec![invalid(lowered.span, why).diagnostic(&info.name)];
    };
    let mut checking = Checking {
        fields: &lowered.fields,
        used: vec![false; lowered.fields.len()],
        span: lowered.span,
        data: &info.data,
        laws,
        program,
        lemmas,
        out: Vec::new(),
    };
    checking.class(bundle, subject, &mut Vec::new());
    let Checking { used, mut out, .. } = checking;
    for (field, _) in lowered.fields.iter().zip(used).filter(|(_, used)| !used) {
        let why = format!("`{}` is no law of the classes of the bundle", field.name);
        out.push(invalid(field.span, why));
    }
    out.iter().map(|f| f.diagnostic(&info.name)).collect()
}

struct Checking<'a, 'p> {
    fields: &'a [Field],
    used: Vec<bool>,
    span: Span,
    data: &'a logic::Data,
    laws: Laws<'a, 'p>,
    program: &'a dyn logic::Program,
    lemmas: &'a [logic::Lemma],
    out: Vec<Failure>,
}

impl Checking<'_, '_> {
    /// The laws of `bundle`'s class at `dict`, under the superclass fields
    /// `path`, then those of its superclasses.
    fn class(&mut self, bundle: &Bundle, dict: &Dict, path: &mut Vec<String>) {
        let Some(laws) = (self.laws)(&bundle.module, &bundle.class) else {
            // a class with laws unread would be certified of none
            self.out.push(Failure::Refused(logic::Refused {
                span: self.span,
                why: format!(
                    "the laws of `{}` are not available to this proof",
                    bundle.class
                ),
            }));
            return;
        };
        for law in laws {
            let name = &law.1.name;
            let field = (self.fields.iter()).position(|f| f.path == *path && f.name == *name);
            let Some(i) = field else {
                let why = format!(
                    "the bundle has no lemma for the law `{name}` of `{}`",
                    bundle.class
                );
                self.out.push(invalid(self.span, why));
                continue;
            };
            self.used[i] = true;
            if let Err(failure) = self.law(law, bundle, dict, &self.fields[i]) {
                self.out.push(failure);
            }
        }
        let slots: Vec<&String> = bundle.supers.iter().map(|(s, _)| s).collect();
        let shared = (slots.iter().enumerate()).find(|(i, s)| slots[..*i].contains(s));
        if let Some((_, slot)) = shared {
            // a field names no one superclass, nor its dictionary one instance
            self.out.push(Failure::Refused(logic::Refused {
                span: self.span,
                why: format!(
                    "two superclasses of `{}` share the dictionary field `{slot}`",
                    bundle.class
                ),
            }));
            // the fields under it are refused with it, not reported again
            for (field, used) in self.fields.iter().zip(&mut self.used) {
                *used |= field.path.len() > path.len() && field.path.starts_with(path);
            }
            return;
        }
        for (slot, inner) in &bundle.supers {
            let dict = Dict::Super {
                dict: Box::new(dict.clone()),
                class: slot.clone(),
            };
            path.push(slot.clone());
            self.class(inner, &dict, path);
            path.pop();
        }
    }

    /// The law `law` of `bundle`'s class at `dict`, by the lemma of `field`.
    fn law(
        &self,
        (law, info): &Proven,
        bundle: &Bundle,
        dict: &Dict,
        field: &Field,
    ) -> Result<(), Failure> {
        let params = info.class.as_ref().map_or(&[][..], |c| &c.params[..]);
        let mut at = law.clone();
        for (l, r) in at.sides.iter_mut().flatten() {
            self.given(l, dict);
            self.given(r, dict);
        }
        // the law's own type variables apart from the bundle's, so that the
        // class's parameters filled in capture none of them
        let at_bundle = |t: &Ty| apart(t, params).instantiate(params, &bundle.args);
        let mut stated = info.clone();
        stated.params = info.params.iter().map(at_bundle).collect();
        if let GoalType::Equal(t) = &info.goal {
            stated.goal = GoalType::Equal(at_bundle(t));
        }
        let statement = statement(&at, &stated)?;
        let ExprKind::Global { module, name, .. } = &field.value.kind else {
            return Err(invalid(
                field.span,
                "a law's field names the proof of it".into(),
            ));
        };
        let lemma =
            (self.lemmas.iter()).find(|l| module == logic::LEMMAS && l.statement.name == *name);
        let Some(lemma) = lemma else {
            let why = format!("`{name}` is no proof checked before this one");
            return Err(invalid(field.span, why));
        };
        let order: Vec<bool> = law.premise.clone();
        let types: Vec<&Ty> = statement.params.iter().map(|(_, t)| t).collect();
        let theirs: Vec<&Ty> = lemma.statement.params.iter().map(|(_, t)| t).collect();
        // the lemma is about values of exactly the law's types: its
        // parameters, premises and equation, up to a renaming
        let mut renaming = Vec::new();
        let equations = |s: &logic::Statement| -> Vec<Ty> {
            let goal = match &s.goal {
                logic::Goal::Equal(e) => Some(e.ty.clone()),
                logic::Goal::SameType(..) => None,
            };
            (s.premises.iter().map(|(_, e)| e.ty.clone()))
                .chain(goal)
                .collect()
        };
        let (ours, others) = (equations(&statement), equations(&lemma.statement));
        let fits = lemma
            .order
            .iter()
            .map(|(_, p)| *p)
            .eq(order.iter().copied())
            && types.len() == theirs.len()
            && types
                .iter()
                .zip(&theirs)
                .all(|(a, b)| renames(b, a, &mut renaming))
            && ours.len() == others.len()
            && (ours.iter().zip(&others)).all(|(a, b)| renames(b, a, &mut renaming));
        if !fits {
            let why = format!(
                "`{name}` is not stated over the parameters of the law `{}`",
                info.name
            );
            return Err(invalid(field.span, why));
        }
        let args = (law.params.iter())
            .zip(&law.premise)
            .map(|(x, premise)| {
                let x: Rc<str> = x.as_str().into();
                if *premise {
                    Arg::Proof(Cert {
                        kind: Rule::Fact(x),
                        span: field.span,
                    })
                } else {
                    Arg::Value(Term::Var(x))
                }
            })
            .collect();
        let cert = Cert {
            kind: Rule::Lemma(name.as_str().into(), args),
            span: field.span,
        };
        let mut data = info.data.clone();
        data.extend(self.data);
        logic::prove(&statement, &cert, &data, self.program, self.lemmas)
    }

    /// `e` with the law's given dictionary `dict`: each superclass of it the
    /// instance it is, where the instance says.
    fn given(&self, e: &mut Expr, dict: &Dict) {
        if let ExprKind::Method { dict: d, .. } = &mut e.kind {
            *d = self.canonical(at(d, dict));
        }
        e.each_child_mut(&mut |c| self.given(c, dict));
    }

    fn canonical(&self, d: Dict) -> Dict {
        let Dict::Super { dict, class } = d else {
            return d;
        };
        let inner = self.canonical(*dict);
        if let Dict::Instance { module, name, args } = &inner
            && args.is_empty()
            && let Some((info, _)) = self.program.instance(module, name)
            && let mut found = info.supers.iter().filter(|(c, _)| *c == class)
            && let (Some((_, own)), None) = (found.next(), found.next())
        {
            return self.canonical(own.clone());
        }
        Dict::Super {
            dict: Box::new(inner),
            class,
        }
    }
}

/// `d` with the given dictionary it stands on replaced by `dict`.
fn at(d: &Dict, dict: &Dict) -> Dict {
    match d {
        Dict::Given(_) => dict.clone(),
        Dict::Super { dict: inner, class } => Dict::Super {
            dict: Box::new(at(inner, dict)),
            class: class.clone(),
        },
        other => other.clone(),
    }
}

/// `t` with each rigid variable but the class's parameters `keep` renamed
/// apart from any of a bundle's types.
fn apart(t: &Ty, keep: &[String]) -> Ty {
    match t {
        Ty::Rigid(x) if !keep.contains(x) => Ty::Rigid(format!("{x}$law")),
        Ty::Data { module, name, args } => Ty::Data {
            module: module.clone(),
            name: name.clone(),
            args: args.iter().map(|a| apart(a, keep)).collect(),
        },
        Ty::Fun(a, b) => Ty::Fun(Box::new(apart(a, keep)), Box::new(apart(b, keep))),
        Ty::Rigid(_) | Ty::Bool | Ty::Int | Ty::Refused(_) => t.clone(),
    }
}

/// Whether `ours` is `theirs` with its rigid variables renamed one to one.
fn renames(ours: &Ty, theirs: &Ty, renaming: &mut Vec<(String, String)>) -> bool {
    match (ours, theirs) {
        (Ty::Rigid(mine), Ty::Rigid(other)) => {
            if let Some((was, is)) = (renaming.iter()).find(|(was, is)| was == mine || is == other)
            {
                return was == mine && is == other;
            }
            renaming.push((mine.clone(), other.clone()));
            true
        }
        (
            Ty::Data { module, name, args },
            Ty::Data {
                module: other_module,
                name: other_name,
                args: other_args,
            },
        ) => {
            module == other_module
                && name == other_name
                && args.len() == other_args.len()
                && (args.iter().zip(other_args)).all(|(mine, other)| renames(mine, other, renaming))
        }
        (Ty::Fun(param, result), Ty::Fun(other_param, other_result)) => {
            renames(param, other_param, renaming) && renames(result, other_result, renaming)
        }
        (Ty::Bool, Ty::Bool) | (Ty::Int, Ty::Int) => true,
        _ => false,
    }
}
