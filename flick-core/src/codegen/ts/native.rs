use super::projections::Projected;
use super::{Piece, Projection, Type, Vars};

impl Projection<'_> {
    /// `form`, the TypeScript form a module gives its type `name`, applied
    /// to `args` on the selected runtime; `None` when the type is not given
    /// all its arguments.
    pub(super) fn native_applied(
        &self,
        form: &Projected,
        args: &[&Type],
        vars: &Vars,
        name: &str,
    ) -> Result<Option<String>, String> {
        let Some((effect, rt)) = &form.form else {
            return Err(format!("`{name}` has no TypeScript form"));
        };
        if args.len() != form.arity {
            return Ok(None);
        }
        let mut out = String::new();
        for piece in if self.rt { rt } else { effect } {
            match piece {
                Piece::Text(text) => out.push_str(text),
                Piece::Place { index, row: true } => out.push_str(&self.row(args[*index], vars)?),
                Piece::Place { index, row: false } => out.push_str(&self.ty(args[*index], vars)?),
            }
        }
        self.imported
            .borrow_mut()
            .extend(form.imports.iter().cloned());
        Ok(Some(out))
    }
}
