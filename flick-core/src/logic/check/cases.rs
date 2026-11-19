//! Case analysis: a split of a variable of the statement into each case of
//! its type, the goal proved in every one with the variable refined.

use std::rc::Rc;

use flick_ir::Lit;

use super::{Checker, invalid};
use crate::logic::eval::Value;
use crate::logic::term::{Literal, Pat};
use crate::logic::{Cert, Equation, Failure, Ty};
use crate::source::Span;

impl Checker<'_> {
    pub(super) fn case(
        &mut self,
        goal: &Equation,
        v: &Rc<str>,
        arms: &[(Pat, Cert)],
        span: Span,
    ) -> Result<(), Failure> {
        let Some((_, ty)) = self.vars.iter().rev().find(|(n, _)| n == v).cloned() else {
            return invalid(span, format!("`{v}` is no variable of the statement"));
        };
        if self.refined.iter().any(|(n, _)| n == v) {
            return invalid(span, format!("`{v}` is already split"));
        }
        let cases: Vec<(Pat, Vec<Ty>)> = match &ty {
            Ty::Bool => [true, false]
                .map(|b| (Pat::Lit(Literal(Lit::Bool(b))), Vec::new()))
                .into(),
            Ty::Data { module, name, args } => {
                self.admit_data(module, name, span)?;
                let decl = self
                    .data
                    .get(&(module.clone(), name.clone()))
                    .expect("admitted above");
                decl.constructors
                    .iter()
                    .map(|(c, fields)| {
                        let fields = fields
                            .iter()
                            .map(|f| f.instantiate(&decl.params, args))
                            .collect();
                        (Pat::Bind(c.as_str().into()), fields)
                    })
                    .collect()
            }
            _ => {
                return invalid(
                    span,
                    format!("`{v}` is of `{ty}`, which has no constructors to split"),
                );
            }
        };
        let mut covered = vec![false; cases.len()];
        for (pat, proof) in arms {
            let (index, binders) = self.arm(pat, &cases, &covered, proof.span)?;
            let Some(index) = index else {
                // a variable or `_`: every case not covered yet, with `v` as it is
                covered.fill(true);
                self.check(goal, proof)?;
                continue;
            };
            covered[index] = true;
            let mark = self.vars.len();
            let mut fields = Vec::new();
            for (b, field_ty) in binders.into_iter().zip(&cases[index].1) {
                self.bind(&b, field_ty, proof.span)?;
                fields.push(Value::Sym(b));
            }
            let value = match pat {
                Pat::Lit(l) => Value::Lit(l.clone()),
                Pat::Con(c, _) => Value::Con(c.clone(), fields.into()),
                _ => unreachable!("an arm of a case is a constructor or a literal"),
            };
            self.refined.push((v.clone(), value));
            let result = self.check(goal, proof);
            self.refined.pop();
            self.vars.truncate(mark);
            result?;
        }
        if let Some(i) = covered.iter().position(|c| !c) {
            return invalid(
                span,
                format!("the split of `{v}` misses `{}`", case_name(&cases[i].0)),
            );
        }
        Ok(())
    }

    /// Which case an arm proves, and the names of its fields.
    fn arm(
        &mut self,
        pat: &Pat,
        cases: &[(Pat, Vec<Ty>)],
        covered: &[bool],
        span: Span,
    ) -> Result<(Option<usize>, Vec<Rc<str>>), Failure> {
        let index = match pat {
            Pat::Wild | Pat::Bind(_) => return Ok((None, Vec::new())),
            Pat::Lit(l) => cases.iter().position(|(p, _)| *p == Pat::Lit(l.clone())),
            Pat::Con(c, _) => cases.iter().position(|(p, _)| case_name(p) == c.name),
        };
        let Some(index) = index else {
            return invalid(span, "this arm is no case of the variable's type");
        };
        if covered[index] {
            return invalid(
                span,
                format!("`{}` is split twice", case_name(&cases[index].0)),
            );
        }
        let mut binders = Vec::new();
        if let Pat::Con(_, fields) = pat {
            for field in fields {
                binders.push(match field {
                    Pat::Bind(b) => b.clone(),
                    Pat::Wild => {
                        self.fresh += 1;
                        format!("$field{}", self.fresh).into()
                    }
                    _ => return invalid(span, "a split names each field: `C x y`, nested patterns are a split of a field"),
                });
            }
        }
        Ok((Some(index), binders))
    }
}

fn case_name(p: &Pat) -> String {
    match p {
        Pat::Bind(c) => c.to_string(),
        Pat::Lit(Literal(Lit::Bool(b))) => b.to_string(),
        _ => "a case".into(),
    }
}
