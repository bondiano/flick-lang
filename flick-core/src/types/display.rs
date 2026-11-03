//! Types as diagnostics show them, in source syntax.

use std::fmt::Write;

use super::Checker;
use super::ty::{Binder, Row, Type};
use super::unify::Store;
use crate::numeric::default_num;

impl Checker<'_> {
    pub fn show(&self, t: &Type) -> String {
        let mut out = String::new();
        let printer = Printer {
            store: &self.store,
            qualified: false,
        };
        printer.print(&self.held_now(&self.zonk(t)), Prec::Top, &mut out);
        out
    }

    /// `t` with each constructor written with its module, which tells two
    /// types of one name apart.
    pub fn show_qualified(&self, t: &Type) -> String {
        let mut out = String::new();
        let printer = Printer {
            store: &self.store,
            qualified: true,
        };
        printer.print(&self.zonk(t), Prec::Top, &mut out);
        out
    }

    /// `t` with each qualifier as what it holds now.
    fn held_now(&self, t: &Type) -> Type {
        match t {
            Type::Captured {
                ty,
                captures,
                value,
            } => Type::Captured {
                ty: Box::new(self.held_now(ty)),
                captures: Box::new(self.expanded(captures)),
                value: *value,
            },
            Type::Bounded { ty, captures } => Type::Bounded {
                ty: Box::new(self.held_now(ty)),
                captures: Box::new(self.expanded(captures)),
            },
            _ => t.map_children(&mut |child| self.held_now(child)),
        }
    }
}

/// A closed type, as an interface stores it, in source syntax.
pub fn show_type(t: &Type) -> String {
    let store = Store::default();
    let mut out = String::new();
    Printer {
        store: &store,
        qualified: false,
    }
    .print(t, Prec::Top, &mut out);
    out
}

struct Printer<'s> {
    store: &'s Store,
    qualified: bool,
}

impl Printer<'_> {
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
            Type::Con { module, name } if self.qualified && !module.is_empty() => {
                let _ = write!(out, "{module}.{name}");
            }
            Type::Con { name, .. } | Type::Var(name) => out.push_str(name),
            Type::Symbol(label) => {
                let _ = write!(out, "{label:?}");
            }
            Type::Skolem(s) => out.push_str(self.store.skolem_name(*s)),
            // a number literal's type is `Int`, or `Float` with a fraction,
            // unless something says otherwise
            Type::Meta(m) if self.store.is_numeric(*m) => {
                out.push_str(default_num(self.store.is_fractional(*m)).name());
            }
            Type::Meta(_) => out.push('_'),
            Type::Error => out.push('?'),
            Type::App { .. } => {
                let (head, args) = t.spine();
                parens(prec >= Prec::Arg, out, &mut |out| {
                    self.print(head, Prec::App, out);
                    for arg in &args {
                        out.push(' ');
                        self.print(arg, Prec::Arg, out);
                    }
                });
            }
            Type::Fun {
                param,
                effects,
                result,
            } => parens(prec >= Prec::App, out, &mut |out| {
                self.print(param, Prec::App, out);
                match effects {
                    None => out.push_str(" -> "),
                    Some(e) if is_empty(&e.services) && is_empty(&e.errors) => {
                        out.push_str(" ->{} ");
                    }
                    Some(e) => {
                        out.push_str(" ->{");
                        self.print_row_items(&e.services, out);
                        out.push_str("; ");
                        self.print_row_items(&e.errors, out);
                        out.push_str("} ");
                    }
                }
                self.print(result, Prec::Top, out);
            }),
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
                if row
                    .entries
                    .iter()
                    .all(|(l, _)| l.contains('.') || l.starts_with(['^', '~'])) =>
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
        let Type::Row(row) = t else {
            self.print(t, Prec::App, out);
            return;
        };
        if row.entries.is_empty() && row.tail.is_none() {
            out.push_str("none");
            return;
        }
        for (i, (_, item)) in row.entries.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            self.print(item, Prec::App, out);
        }
        if let Some(tail) = &row.tail {
            out.push_str(if row.entries.is_empty() { "" } else { " | " });
            self.print(tail, Prec::App, out);
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
    use crate::types::lang::Lang;

    #[test]
    fn show_type_leaves_out_a_capture_still_being_inferred() {
        let t = Type::Captured {
            ty: Box::new(Type::lang(Lang::Int)),
            captures: Box::new(Type::Meta(207)),
            value: false,
        };
        assert_eq!(show_type(&t), "Int");
    }
}
