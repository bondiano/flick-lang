//! Types marked `@cell`: a value of one holds one value, as a state made by
//! a pure call does. A binding built purely is generalized, so a cell whose
//! type has a variable would be one cell that each reader takes for its own
//! type. A value that is no function therefore keeps the types inside a
//! cell's from being generalized, and one that gets them from a signature
//! is refused (`FLICK-T072`); a plugin that watches the tag is told, and
//! may refuse in the words of its library.

use std::collections::HashMap;

use flick_plugin::{Binding, CallSite, Tagged};

use super::ty::Type;
use super::{Checker, makers};
use crate::diagnostics::codes;
use crate::iface;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// The types of this module marked `@cell`, and those of the modules
    /// whose interfaces it knows: an interface names its own, and passes on
    /// the cells of other modules that its exports mention, as an importer
    /// may not import those.
    pub fn declare_cells(&mut self, decls: &[&Node]) {
        self.typing.cells = makers::marked(decls, self.text, iface::CELL, K::Con);
        let mut cells = HashMap::from([(self.env.module.clone(), self.typing.cells.clone())]);
        for interface in self.env.known_interfaces() {
            for (name, of) in interface.tags.get(iface::CELL).into_iter().flatten() {
                let module = of.first().unwrap_or(&interface.module);
                cells.entry(module.clone()).or_default().push(name.clone());
            }
        }
        self.cells = cells;
    }

    /// Whether `module.name` is a type marked `@cell`.
    pub(super) fn cell(&self, module: &str, name: &str) -> bool {
        (self.cells.get(module)).is_some_and(|cells| cells.iter().any(|cell| cell == name))
    }

    /// Keeps the unknowns inside the types of cells in `ty` from being
    /// generalized: one cell holds one value.
    pub fn lower_cells(&mut self, ty: &Type) {
        let ty = self.zonk(ty);
        if !ty.contains(&mut |t| matches!(t, Type::Meta(_))) {
            return;
        }
        let mut held = Vec::new();
        // an alias may stand for a cell
        self.unaliased(&ty).visit(&mut |t| {
            if let (Type::Con { module, name }, args) = t.spine()
                && self.cell(module, name)
            {
                held.extend(args.into_iter().cloned());
            }
        });
        for t in &held {
            self.store.lower(t);
        }
    }

    /// A value of the type `scheme`, not a function, holding a cell whose
    /// type has a variable the scheme quantifies: one cell would hold a value
    /// of every type the variable stands for, each reader taking it for its own.
    pub fn polymorphic_cell(&mut self, name: Option<&str>, scheme: &Type, span: Span) {
        let ty = match scheme {
            Type::Forall { body, .. } => body,
            ty => ty,
        };
        if matches!(self.whnf(ty), Type::Fun { .. }) {
            return;
        }
        // a variable of a scheme is one it quantifies, written or not
        let quantified = |t: &Type| t.contains(&mut |v| matches!(v, Type::Var(_)));
        if !quantified(ty) {
            return;
        }
        let mut cell = None;
        // an alias may stand for a cell
        self.unaliased(ty).visit(&mut |t| {
            if let (Type::Con { module, name }, args) = t.spine()
                && cell.is_none()
                && self.cell(module, name)
                && args.into_iter().any(quantified)
            {
                cell = Some((module.clone(), name.clone()));
            }
        });
        let Some((module, cell)) = cell else {
            return;
        };
        let shown = self.show(ty);
        let (value, it) = name.map_or_else(
            || ("this value".to_owned(), "it".to_owned()),
            |n| (format!("`{n}`"), format!("`{n}`")),
        );
        self.error(
            codes::T072,
            span,
            format!("{value} has the type `{shown}` for every type its variables stand for, yet a `{cell}` holds one value: give {it} a type without variables"),
        );
        if self.options.watched.tag(iface::CELL) {
            let at = flick_plugin::Span {
                start: span.start,
                end: span.end,
            };
            self.typing.call_sites.push(CallSite {
                binding: Some(Binding {
                    module: self.env.module.clone(),
                    name: name.map(str::to_owned),
                    span: at,
                    constrained: false,
                    in_function: false,
                    applied: 0,
                    gives: Some(self.fact_type(ty)),
                }),
                tagged: Some(Tagged {
                    tag: iface::CELL.to_owned(),
                    named: shown,
                    reaches: format!("{module}.{cell}"),
                    inside: Vec::new(),
                }),
                module,
                name: cell,
                span: at,
                arguments: Vec::new(),
                refinement: None,
                generated: span.start >= self.generated,
                lacking: None,
            });
        }
    }
}
