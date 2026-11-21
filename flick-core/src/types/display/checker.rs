//! Checker-aware type display: zonking, live/widened stripping, and naming
//! type constructors as the current module can write them in source syntax.

use super::{Naming, Prec, Printer, is_empty};
use crate::iface::Namespace;
use crate::types::Checker;
use crate::types::env::Found;
use crate::types::regions::{has_live, strip_elided, strip_live};
use crate::types::shown::short_form;
use crate::types::ty::{Row, Type};

impl Checker<'_> {
    /// `t` for a message: as a reader wants a declared or inferred type, without
    /// what a signature gets back (`shown`). Items inside service and error rows
    /// are named the way this module can name them.
    pub fn show(&self, t: &Type) -> String {
        let mut out = String::new();
        let printer = Printer {
            store: &self.store,
            naming: Naming::RowInScope(self),
            zero: true,
        };
        let held = self.held_now(&self.unlive(t));
        printer.print(&short_form(&held, &self.store), Prec::Top, &mut out);
        out
    }

    /// `t` as this module can write it in source: every type constructor and
    /// omitted row element named as it resolves in scope.
    pub fn show_written(&self, t: &Type) -> String {
        let mut out = String::new();
        let printer = Printer {
            store: &self.store,
            naming: Naming::InScope(self),
            zero: true,
        };
        let held = self.held_now(&self.unlive(t));
        printer.print(&short_form(&held, &self.store), Prec::Top, &mut out);
        out
    }

    /// `t` with its `forall` and qualifiers, every type constructor named as
    /// this module can write it in source.
    pub fn show_exact_written(&self, t: &Type) -> String {
        let mut out = String::new();
        let printer = Printer {
            store: &self.store,
            naming: Naming::InScope(self),
            zero: false,
        };
        printer.print(&self.held_now(&self.unlive(t)), Prec::Top, &mut out);
        out
    }

    /// How this module writes the type constructor `(module, name)`: plain
    /// when `name` already resolves to `(module, name)` in scope, otherwise
    /// qualified through an opened witness, import qualifier, built-in module,
    /// or child module that exports it.
    pub(in crate::types) fn written_type_name(&self, module: &str, name: &str) -> String {
        if let Some(binder) = module.strip_prefix("$exists:") {
            return format!("{binder}.{name}");
        }
        if let Some(class) = name.strip_suffix("$Dict") {
            return format!("{}.Dict", self.written_type_name(module, class));
        }
        if let Some(service) = name.strip_suffix(".Impl") {
            return format!("{}.Impl", self.written_type_name(module, service));
        }
        if module.is_empty() || name.contains('.') {
            return name.to_owned();
        }
        if let Some((_, ty, _)) = self
            .contract_types
            .iter()
            .rev()
            .find(|(local, _, _)| local == name)
        {
            if matches!(ty, Type::Con { module: m, name: n } if m == module && n == name) {
                return name.to_owned();
            }
        } else if matches!(
            self.env.find(Namespace::Type, name),
            Found::Entity(e) if e.module == module && e.name == name
        ) {
            return name.to_owned();
        }
        for (i, opened) in self.opened_modules.iter().enumerate().rev() {
            if opened.alias.is_empty()
                || self.opened_modules[i + 1..]
                    .iter()
                    .any(|later| later.alias == opened.alias)
            {
                continue;
            }
            if let Some((member, _, _)) = opened.types.iter().find(|(_, ty, _)| {
                matches!(ty, Type::Con { module: m, name: n } if m == module && n == name)
            }) {
                return format!("{}.{member}", opened.alias);
            }
        }
        let shadowed = |q: &str| self.opened_modules.iter().any(|o| o.alias == q);
        match self.env.type_qualifier(module, name, &shadowed) {
            Some(q) => format!("{q}.{name}"),
            None => name.to_owned(),
        }
    }

    /// `t` zonked, without the `Live` of `regions/live.rs` or unsolved
    /// subsumption-widening tails: no row writes either.
    fn unlive(&self, t: &Type) -> Type {
        let t = strip_elided(&self.zonk(t));
        let t = if has_live(&t) { strip_live(&t) } else { t };
        self.strip_widened(&t)
    }

    fn strip_widened(&self, t: &Type) -> Type {
        match t {
            Type::Meta(meta)
                if meta
                    .inference_type_root()
                    .is_some_and(|m| self.store.is_widened_row(m)) =>
            {
                Type::empty_row()
            }
            Type::Row(row) => {
                let entries = row
                    .entries
                    .iter()
                    .map(|(l, t)| (l.clone(), self.strip_widened(t)))
                    .collect();
                let tail = row
                    .tail
                    .as_deref()
                    .map(|t| self.strip_widened(t))
                    .filter(|t| !is_empty(t))
                    .map(Box::new);
                Type::Row(Row {
                    entries,
                    tail,
                    written: row.written,
                })
            }
            Type::Union(a, b) => {
                let (a, b) = (self.strip_widened(a), self.strip_widened(b));
                if is_empty(&a) {
                    b
                } else if is_empty(&b) {
                    a
                } else {
                    Type::Union(Box::new(a), Box::new(b))
                }
            }
            _ => t.map_children(&mut |c| self.strip_widened(c)),
        }
    }

    /// `t` with its `forall` and its qualifiers, for a message about them and
    /// for text that is not a message: a dictionary's name, a plugin's fact.
    pub fn show_exact(&self, t: &Type) -> String {
        let mut out = String::new();
        let printer = Printer {
            store: &self.store,
            naming: Naming::Short,
            zero: false,
        };
        printer.print(&self.held_now(&self.unlive(t)), Prec::Top, &mut out);
        out
    }

    /// `t` with each constructor written with its module, which tells two
    /// types of one name apart.
    pub fn show_qualified(&self, t: &Type) -> String {
        let mut out = String::new();
        let printer = Printer {
            store: &self.store,
            naming: Naming::Qualified,
            zero: false,
        };
        printer.print(&self.unlive(t), Prec::Top, &mut out);
        out
    }

    /// `t` with each qualifier as what it holds now.
    fn held_now(&self, t: &Type) -> Type {
        match t {
            Type::Captured {
                ty,
                captures,
                value,
                elided,
            } => Type::Captured {
                ty: Box::new(self.held_now(ty)),
                captures: Box::new(self.expanded(captures)),
                value: *value,
                elided: *elided,
            },
            Type::Bounded { ty, captures } => Type::Bounded {
                ty: Box::new(self.held_now(ty)),
                captures: Box::new(self.expanded(captures)),
            },
            _ => t.map_children(&mut |child| self.held_now(child)),
        }
    }
}
