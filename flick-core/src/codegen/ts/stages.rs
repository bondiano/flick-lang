//! Type parameters of a curried function, each on the stage that introduces
//! it: `<A>(arg: A) => <B>(arg: B) => readonly [A, B]`, so a later argument
//! still infers its type. The JavaScript is the same; only the type moves.

use super::{Projection, Vars, uncaptured};
use crate::types::{Effects, Type};

impl Projection<'_> {
    /// The function `body` with each of `generics` (its Flick name and its
    /// TypeScript one) on the first stage whose parameter mentions it; one
    /// only the result mentions goes on the last stage.
    pub(super) fn staged(
        &self,
        body: &Type,
        vars: &Vars,
        generics: &[(String, String)],
    ) -> Result<String, String> {
        // the stages a caller supplies one by one: pure arrows, then the last
        let mut stages = Vec::new();
        let mut t = body;
        while let Type::Fun {
            param,
            effects,
            result,
        } = t
        {
            stages.push((t, &**param));
            // a curried stage closes over the arguments before it
            let next = uncaptured(result, vars)?;
            match (next, effects) {
                (Type::Fun { .. }, None) => t = next,
                _ => break,
            }
        }
        let Some(last) = stages.len().checked_sub(1) else {
            return self.ty(body, vars);
        };
        let mut placed: Vec<Vec<&str>> = vec![Vec::new(); stages.len()];
        for (name, ts) in generics {
            let at = stages
                .iter()
                .position(|(_, param)| mentions(param, name))
                .unwrap_or(last);
            placed[at].push(ts);
        }
        let with = |i: usize, ty: String| {
            if placed[i].is_empty() {
                ty
            } else {
                format!("<{}>{ty}", placed[i].join(", "))
            }
        };
        let mut out = with(last, self.ty(stages[last].0, vars)?);
        for i in (0..last).rev() {
            let param = self.ty(stages[i].1, vars)?;
            out = with(i, format!("(arg: {param}) => {out}"));
        }
        Ok(out)
    }
}

/// Whether `t` mentions the type variable `name`.
fn mentions(t: &Type, name: &str) -> bool {
    let mut found = false;
    t.visit(&mut |c| found |= matches!(c, Type::Var(v) if v == name));
    found
}

/// `scheme` with its last stage a runtime one when it is Sync: what an
/// exported Sync function is called as from TypeScript.
pub fn exported(scheme: &Type) -> Type {
    match scheme {
        Type::Forall {
            vars,
            constraints,
            body,
        } => Type::Forall {
            vars: vars.clone(),
            constraints: constraints.clone(),
            body: Box::new(exported(body)),
        },
        Type::Captured {
            ty,
            captures,
            value,
            elided,
        } => Type::Captured {
            ty: Box::new(exported(ty)),
            captures: captures.clone(),
            value: *value,
            elided: *elided,
        },
        Type::Bounded { ty, captures } => Type::Bounded {
            ty: Box::new(exported(ty)),
            captures: captures.clone(),
        },
        Type::Fun {
            param,
            effects: None,
            result,
        } => Type::Fun {
            param: param.clone(),
            effects: None,
            result: Box::new(exported(result)),
        },
        Type::Fun {
            param,
            effects: Some(e),
            result,
        } if e.sync => Type::Fun {
            param: param.clone(),
            effects: Some(Box::new(Effects {
                sync: false,
                ..(**e).clone()
            })),
            result: result.clone(),
        },
        other => other.clone(),
    }
}
