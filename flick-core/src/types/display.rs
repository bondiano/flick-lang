//! Types as diagnostics show them, in source syntax.

mod checker;

use crate::numeric::NumLang;
use crate::types::Elem;
use std::fmt::Write;

use super::Checker;
use super::lang::Lang;
use super::regions::{has_live, strip_elided, strip_live};
use super::shown::short_form;
use super::ty::{Binder, Effects, Row, Type};
use super::unify::Store;
use crate::numeric::default_num;

/// A closed type, as an interface stores it, in source syntax.
pub fn show_type(t: &Type) -> String {
    let t = &strip_elided(t);
    let stripped;
    let t = if has_live(t) {
        stripped = strip_live(t);
        &stripped
    } else {
        t
    };
    let store = Store::default();
    let mut out = String::new();
    Printer {
        store: &store,
        naming: Naming::Short,
        zero: false,
    }
    .print(t, Prec::Top, &mut out);
    out
}

/// A declared or inferred type as a reader sees it, without what a signature
/// written that way gets back (`shown`). A parameter or a field on its own
/// keeps its `forall`: use [`show_type`] for those.
pub fn show_signature(t: &Type) -> String {
    let t = &strip_elided(t);
    let stripped;
    let t = if has_live(t) {
        stripped = strip_live(t);
        &stripped
    } else {
        t
    };
    let store = Store::default();
    let mut out = String::new();
    Printer {
        store: &store,
        naming: Naming::Short,
        zero: true,
    }
    .print(&short_form(t, &store), Prec::Top, &mut out);
    out
}

/// `t` with each constructor written with its module.
#[cfg(test)]
pub(super) fn show_type_qualified(t: &Type) -> String {
    let t = &strip_elided(t);
    let store = Store::default();
    let mut out = String::new();
    Printer {
        store: &store,
        naming: Naming::Qualified,
        zero: true,
    }
    .print(t, Prec::Top, &mut out);
    out
}

#[derive(Clone, Copy)]
enum Naming<'s, 'c> {
    /// Unqualified constructor names.
    Short,
    /// Every constructor qualified with its defining module.
    Qualified,
    /// Every constructor and omitted row element named as the current module
    /// can write it in source.
    InScope(&'s Checker<'c>),
    /// Short outside service and error rows; in-scope inside them.
    RowInScope(&'s Checker<'c>),
}

#[derive(Clone, Copy)]
struct Printer<'s, 'c> {
    store: &'s Store,
    naming: Naming<'s, 'c>,
    /// Writes `^{}` on a later stage. The short form has only those that say
    /// something: without it, the stage would hold what its parameters hold.
    zero: bool,
}

impl Printer<'_, '_> {
    fn row_printer(&self) -> Self {
        let naming = match self.naming {
            Naming::RowInScope(checker) => Naming::InScope(checker),
            other => other,
        };
        Printer { naming, ..*self }
    }

    fn print_con(&self, module: &str, name: &str, out: &mut String) {
        match self.naming {
            Naming::Qualified if !module.is_empty() => {
                let _ = write!(out, "{module}.{name}");
            }
            Naming::InScope(checker) => {
                out.push_str(&checker.written_type_name(module, name));
            }
            Naming::Short | Naming::Qualified | Naming::RowInScope(_) => out.push_str(name),
        }
    }

    fn print(&self, t: &Type, prec: Prec, out: &mut String) {
        let parens = |needed: bool, out: &mut String, f: &mut dyn FnMut(&mut String)| {
            if needed {
                out.push('(');
            }
            f(out);
            if needed {
                out.push(')');
            }
        };
        match t {
            Type::Con { module, name } if let Some(binder) = module.strip_prefix("$exists:") => {
                let _ = write!(out, "{binder}.{name}");
            }
            Type::Con { module, name } if name.ends_with("$Dict") => {
                self.print_dictionary_name(module, name, out);
            }
            Type::Con { module, name } => self.print_con(module, name, out),
            Type::Var(name) if name.ends_with("$Dict") => self.print_dictionary_name("", name, out),
            Type::Var(name) => out.push_str(name),
            Type::Symbol(label) => {
                let _ = write!(out, "{label:?}");
            }
            Type::Skolem(s) => out.push_str(self.store.skolem_name(*s)),
            // a number literal's type is `Int`, or `Float` with a fraction,
            // unless something says otherwise
            Type::Meta(meta)
                if meta
                    .inference_type_root()
                    .is_some_and(|m| self.store.is_numeric(m)) =>
            {
                out.push_str(default_num(self.store.is_fractional(meta.root())).name());
            }
            Type::Meta(_) => out.push('_'),
            Type::Error => out.push('?'),
            Type::App { .. } => self.print_app(t, prec, out),
            Type::Fun {
                param,
                effects,
                result,
            } => self.print_fun(param, effects.as_deref(), result, prec, out),
            Type::Tuple(items) => {
                out.push('(');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    self.print(item, Prec::Top, out);
                }
                out.push(')');
            }
            Type::Row(row)
                if row.entries.iter().all(|(l, _)| {
                    l.name().is_none_or(|name| name.contains('.'))
                        && !matches!(l, Elem::Service(_) | Elem::Live | Elem::Provided(_))
                }) =>
            {
                if row.entries.is_empty() && row.tail.is_none() {
                    out.push_str("none");
                } else {
                    out.push('{');
                    self.print_row_items(t, out);
                    out.push('}');
                }
            }
            Type::Record(row) | Type::Row(row) => self.print_fields(row, out),
            Type::Union(a, b) => parens(prec >= Prec::Arg, out, &mut |out| {
                self.print(a, Prec::App, out);
                out.push_str(" + ");
                self.print(b, Prec::App, out);
            }),
            Type::Without { .. } => {
                out.push('{');
                self.print_row_items(t, out);
                out.push('}');
            }
            Type::Captured { ty, captures, .. } | Type::Bounded { ty, captures } => {
                self.print_captured(ty, captures, prec, out);
            }
            Type::Forall {
                vars,
                constraints,
                body,
            } => parens(prec > Prec::Top, out, &mut |out| {
                self.print_forall(vars, constraints, body, out);
            }),
            Type::Exists {
                scope: _,
                binder,
                contract,
                body,
            } => parens(prec > Prec::Top, out, &mut |out| {
                let _ = write!(out, "exists {binder} : ");
                self.print(contract, Prec::Top, out);
                out.push_str(". ");
                self.print(body, Prec::Top, out);
            }),
        }
    }

    fn print_fun(
        &self,
        param: &Type,
        effects: Option<&Effects>,
        result: &Type,
        prec: Prec,
        out: &mut String,
    ) {
        let needs_parens = prec >= Prec::App;
        if needs_parens {
            out.push('(');
        }
        self.print(param, Prec::App, out);
        match effects {
            None => out.push_str(" -> "),
            Some(e) => {
                out.push_str(" ->{");
                if !is_empty(&e.services) {
                    self.print_row_items(&e.services, out);
                }
                if !is_empty(&e.errors) {
                    out.push_str("; ");
                    self.print_row_items(&e.errors, out);
                }
                out.push_str("} ");
            }
        }
        self.print_result(result, out);
        if needs_parens {
            out.push(')');
        }
    }

    fn print_app(&self, t: &Type, prec: Prec, out: &mut String) {
        let (head, args) = t.spine();
        let needs_parens = prec >= Prec::Arg;
        if needs_parens {
            out.push('(');
        }
        self.print(head, Prec::App, out);
        for arg in &args {
            out.push(' ');
            self.print(arg, Prec::Arg, out);
        }
        if needs_parens {
            out.push(')');
        }
    }

    fn print_dictionary_name(&self, module: &str, name: &str, out: &mut String) {
        match self.naming {
            Naming::InScope(checker) if !module.is_empty() => {
                out.push_str(&checker.written_type_name(module, name));
            }
            Naming::Qualified if !module.is_empty() => {
                if let Some(class) = name.strip_suffix("$Dict") {
                    let _ = write!(out, "{module}.{class}.Dict");
                }
            }
            _ => {
                if let Some(class) = name.strip_suffix("$Dict") {
                    let _ = write!(out, "{class}.Dict");
                }
            }
        }
    }

    /// What an arrow gives, which may be a later stage that holds nothing.
    fn print_result(&self, result: &Type, out: &mut String) {
        match result {
            Type::Captured { ty, captures, .. }
                if self.zero && is_empty(captures) && matches!(**ty, Type::Fun { .. }) =>
            {
                self.print(ty, Prec::Arg, out);
                out.push_str("^{}");
            }
            _ => self.print(result, Prec::Top, out),
        }
    }

    /// `T^{s}`, `T^c`, `T^(c + d)`; a qualifier still being inferred tells
    /// nothing yet and is left out.
    fn print_captured(&self, ty: &Type, captures: &Type, prec: Prec, out: &mut String) {
        let norm = self.store.normalize(captures);
        if norm.entries.is_empty() && norm.tails.iter().all(|t| matches!(t, Type::Meta(_))) {
            self.print(ty, prec, out);
            return;
        }
        self.print(ty, Prec::Arg, out);
        out.push('^');
        match captures {
            Type::Row(row) if row.entries.is_empty() && row.tail.is_some() => {
                self.print(captures, Prec::Arg, out);
            }
            Type::Row(_) => {
                out.push('{');
                self.print_row_items(captures, out);
                out.push('}');
            }
            other => self.print(other, Prec::Arg, out),
        }
    }

    fn print_forall(&self, vars: &[Binder], constraints: &[Type], body: &Type, out: &mut String) {
        if !vars.is_empty() {
            out.push_str("forall");
            for v in vars {
                let _ = write!(out, " {}", v.name);
            }
            out.push_str(". ");
        }
        match constraints {
            [] => {}
            [one] => {
                self.print(one, Prec::App, out);
                out.push_str(" => ");
            }
            many => {
                out.push('(');
                for (i, c) in many.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    self.print(c, Prec::Top, out);
                }
                out.push_str(") => ");
            }
        }
        self.print(body, Prec::Top, out);
    }

    /// The items of a service or error row without braces: `Users, Log | r`.
    fn print_row_items(&self, t: &Type, out: &mut String) {
        let printer = self.row_printer();
        let row = match t {
            Type::Row(row) => row,
            // `-S` before the `|`, as it is written
            Type::Without { row, gone } => {
                printer.print_gone(gone, true, out);
                out.push_str(" | ");
                printer.print(row, Prec::App, out);
                return;
            }
            _ => {
                printer.print(t, Prec::App, out);
                return;
            }
        };
        if row.entries.is_empty() && row.tail.is_none() {
            out.push_str("none");
            return;
        }
        // a set holds its errors in `Fail`; it reads as it is written, `{A; E}`
        let is_fail = |item: &Type| item.spine().0.is_lang(Lang::Fail);
        let items = row.entries.iter().map(|(_, item)| item);
        for (i, item) in items.clone().filter(|item| !is_fail(item)).enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            printer.print(item, Prec::App, out);
        }
        let services = items.clone().any(|item| !is_fail(item));
        let tail = match row.tail.as_deref() {
            Some(Type::Without { row, gone }) => {
                printer.print_gone(gone, !services, out);
                out.push_str(" | ");
                Some(&**row)
            }
            Some(tail) => {
                out.push_str(if services { " | " } else { "" });
                Some(tail)
            }
            None => None,
        };
        if let Some(tail) = tail {
            printer.print(tail, Prec::App, out);
        }
        for item in items.filter(|item| is_fail(item)) {
            if let [errors] = item.spine().1[..] {
                out.push_str("; ");
                printer.print_row_items(errors, out);
            }
        }
    }

    /// `-A, -B`: the names a row variable is taken without, after the items
    /// before them unless they are the `first`.
    fn print_gone(&self, gone: &[Elem], first: bool, out: &mut String) {
        for (i, label) in gone.iter().enumerate() {
            out.push_str(if i == 0 && first { "-" } else { ", -" });
            match (self.naming, label.name().and_then(|n| n.rsplit_once('.'))) {
                (Naming::InScope(checker), Some((module, name))) => {
                    out.push_str(&checker.written_type_name(module, name));
                }
                (Naming::Qualified, Some((module, name))) if !module.is_empty() => {
                    let _ = write!(out, "{module}.{name}");
                }
                _ => out.push_str(&label.short()),
            }
        }
    }

    fn print_fields(&self, row: &Row, out: &mut String) {
        if row.entries.is_empty() && row.tail.is_none() {
            out.push_str("{}");
            return;
        }
        out.push_str("{ ");
        for (i, (label, ty)) in row.entries.iter().enumerate() {
            if i > 0 {
                out.push_str("; ");
            }
            let _ = write!(out, "{label} : ");
            self.print(ty, Prec::Top, out);
        }
        if let Some(tail) = &row.tail {
            out.push_str(if row.entries.is_empty() { "| " } else { " | " });
            self.print(tail, Prec::App, out);
        }
        out.push_str(" }");
    }
}

fn is_empty(t: &Type) -> bool {
    matches!(t, Type::Row(r) if r.entries.is_empty() && r.tail.is_none())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prec {
    Top,
    /// Head of an application or left of an arrow.
    App,
    /// Argument of an application.
    Arg,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Elided;
    use crate::types::lang::Lang;

    #[test]
    fn show_type_leaves_out_a_capture_still_being_inferred() {
        let t = Type::Captured {
            ty: Box::new(Type::lang(Lang::Int)),
            captures: Box::new(Type::meta(207)),
            value: false,
            elided: Elided::default(),
        };
        assert_eq!(show_type(&t), "Int");
    }
}
