//! Types as diagnostics show them, in source syntax.

use std::fmt::Write;

use super::Checker;
use super::ty::{Binder, Row, Type};

impl Checker<'_> {
    pub fn show(&self, t: &Type) -> String {
        let mut out = String::new();
        self.print(&self.zonk(t), Prec::Top, &mut out);
        out
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
            Type::Con { name, .. } | Type::Var(name) => out.push_str(name),
            Type::Skolem(s) => out.push_str(self.store.skolem_name(*s)),
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
            Type::Row(row) if row.entries.iter().all(|(l, _)| l.contains('.')) => {
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
            Type::Forall {
                vars,
                constraints,
                body,
            } => parens(prec > Prec::Top, out, &mut |out| {
                self.print_forall(vars, constraints, body, out);
            }),
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
