use super::{Origin, Piece, Projection, TsForm, Type, Vars};

impl Projection<'_> {
    /// `form`, the TypeScript form `module` gives its type `name`, applied
    /// to `args` on the selected runtime; `None` when the type is not given
    /// all its arguments.
    pub(super) fn native_applied(
        &self,
        form: &TsForm,
        module: &str,
        args: &[&Type],
        vars: &Vars,
        name: &str,
    ) -> Result<Option<String>, String> {
        let Some(pieces) = &form.form else {
            return Err(format!("`{name}` has no TypeScript form"));
        };
        if args.len() != form.arity {
            return Ok(None);
        }
        // an export of a module a package names is apart from every other
        // module's of that name; one of the runtime is the same everywhere
        let apart = |export: &str| {
            (form.imports.iter()).any(|i| i.name == export && matches!(i.from, Origin::Module(_)))
        };
        let mut out = String::new();
        for piece in pieces {
            match piece {
                Piece::Text(text) => out.push_str(text),
                Piece::Name(export) => {
                    out.push('$');
                    out.push_str(export);
                    if apart(export) {
                        out.push_str(&Self::alias(module));
                    }
                }
                Piece::Place { index, row: true } => out.push_str(&self.row(args[*index], vars)?),
                Piece::Place { index, row: false } => out.push_str(&self.ty(args[*index], vars)?),
            }
        }
        let mut imported = self.imported.borrow_mut();
        for import in &form.imports {
            let of = if apart(&import.name) { module } else { "" };
            let named = (import.clone(), of.to_owned());
            if let Err(at) = imported.binary_search(&named) {
                imported.insert(at, named);
            }
        }
        Ok(Some(out))
    }
}
