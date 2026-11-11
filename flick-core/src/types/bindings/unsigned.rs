//! Inference and generalization of unsigned bindings.
use super::{Checker, LetParts, Node, Type, codes};
use std::mem;

impl Checker<'_> {
    pub fn unsigned(&mut self, decl: &Node, parts: &LetParts, name: &str) -> Type {
        let function = parts.is_function();
        let header = self.header_types(decl, parts, false);
        if parts.rec && !parts.is_function() {
            self.error(
                codes::T012,
                self.span(decl),
                format!(
                    "`{name}` would be evaluated while it is being defined; only functions can be recursive"
                ),
            );
        }
        self.store.level += 1;
        let saved = mem::take(&mut self.cx.runtime);
        let signed = mem::take(&mut self.gadt.signed);
        let errors = self.diagnostics.len();
        let mut redundant = None;
        let ty = match (parts.params.is_empty(), parts.header) {
            (false, _) => self.function(&parts.params, parts.body, None, decl),
            (true, Some(header)) => {
                let ty = self.annotation(header);
                if let Some(b) = parts.body
                    && self.check_exact(b, &ty)
                {
                    redundant = Some(header);
                }
                ty
            }
            (true, None) => parts.body.map_or(Type::Error, |b| self.infer(b)),
        };
        let runtime = mem::replace(&mut self.cx.runtime, saved);
        self.gadt.signed = signed;
        self.store.level -= 1;
        let is_fun = matches!(self.whnf(&ty), Type::Fun { .. });
        // a plugin's code needs no signature: no one reads it. Nor does a local
        // function: nothing outside its body sees it, and it stays of one type
        let local = self.store.level > 0 && !parts.rec;
        let signed = header || local || self.span(decl).start >= self.generated;
        if let (true, Some(written), true) = (is_fun, parts.header, parts.params.is_empty()) {
            let written = self.text_of(written);
            self.error(
                codes::T005,
                self.span(decl),
                format!(
                    "write the type of function `{name}` as a signature above the `let`: `{name} : {written}`"
                ),
            );
        } else if (function || is_fun) && !signed {
            // the suggestion shows the rows the body needs, not their unknowns;
            // only this binding's own: an enclosing row still gathers its calls
            self.close_rows_above(Some(self.store.level));
            self.missing_signature(name, &ty, self.span(decl), errors);
        } else if let Some(header) = redundant {
            self.redundant_annotation(decl, parts, header);
        }
        // a type in a GADT branch may hold by its equalities only
        if runtime || function || !self.store.equalities.is_empty() {
            self.store.lower(&ty);
            ty
        } else {
            // a local cell's uses, all in sight, decide its one type
            if self.store.level > 0 {
                self.lower_cells(&ty);
            }
            let ty = self.generalize(&ty);
            // an annotation was checked as the value's type already
            if let (None, Some(body)) = (parts.header, parts.body) {
                self.polymorphic_cell(Some(name), &ty, self.span(body));
            }
            ty
        }
    }
}
