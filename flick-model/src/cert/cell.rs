//! A cell's constants in a certificate: `Ref.make`, `Ref.get` and `Ref.set`
//! are the core's `.const` at the instance the check took, the content `t`
//! of the cell and its capacity `C`, the bound `a^c` of their schemes; the
//! core checks `⌈t⌉ ⊆ C` (`Const.ok`).

use std::rc::Rc;

use crate::check::Inst;
use crate::syntax::{Const, Term};
use crate::translate::lean::elide::caps;
use crate::types::{Row, Stage, Type};

use super::Unsupported;
use super::term::Cx;

/// The constant of the core a cell's constant is.
pub(super) fn cell(c: &Const) -> Option<&'static str> {
    match c {
        Const::RefMake => Some("refMake"),
        Const::RefGet => Some("refGet"),
        Const::RefSet => Some("refSet"),
        _ => None,
    }
}

impl Cx<'_> {
    /// (Const) of a cell's constant, at the arrow `Const.ty` gives it.
    pub(super) fn cell(&self, c: &Const, e: &Term) -> Result<(String, Type), Unsupported> {
        let name = cell(c).ok_or("no cell's constant")?;
        let (_, insts) =
            (self.node(e)?.instance.as_ref()).ok_or("a cell's constant without its instance")?;
        let (Some(capacity), Some(a)) = (
            insts.iter().find_map(|i| match i {
                Inst::Caps(c) => Some(c),
                _ => None,
            }),
            insts.iter().find_map(|i| match i {
                Inst::Type(t) => Some(t),
                _ => None,
            }),
        ) else {
            return Err("a cell's constant at no capacity".into());
        };
        // the content of a cell `get` and `set` take is bounded already
        let (content, capacity) = match a {
            Type::Bounded(t, c) => ((**t).clone(), c.clone()),
            t => (t.clone(), capacity.clone()),
        };
        let t = Type::Bounded(Rc::new(content), capacity.clone());
        let cell = Type::Ref(Rc::new(t.clone()));
        let none = || Stage::Runtime(Row::none(), Row::none());
        let at = match c {
            Const::RefMake => Type::fun(t.clone(), none(), cell),
            Const::RefGet => Type::fun(cell, none(), t.clone()),
            _ => Type::fun(
                cell,
                Stage::Pure,
                Type::Captured(
                    Rc::new(Type::fun(t.clone(), none(), Type::con("Prelude", "Unit"))),
                    capacity.clone(),
                ),
            ),
        };
        let proof = format!(
            "(Typ.const (c := .{name}) (ι := ⟨{}, .base .unit, {}, Row.none, Row.none⟩) (by cell_ok) (by le_live))",
            self.ty(&t)?,
            caps(&capacity, &self.env)?
        );
        Ok((proof, at))
    }
}
