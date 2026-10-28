//! Type parameters of a curried function, each on the stage that introduces
//! it: `<A>(arg: A) => <B>(arg: B) => readonly [A, B]`, so a later argument
//! still infers its type. The JavaScript is the same; only the type moves.

use super::{Projection, Vars, uncaptured};
use crate::types::Type;

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
