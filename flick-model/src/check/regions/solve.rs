//! The end of a declaration (`docs/formal/regions.md` §2.6): capture rows
//! grow to the least sets their obligations need, then each obligation,
//! `Outlives` (§2.4) and (Err-Raise) (§2.5) is decided.

use super::super::{Check, Checker, error};
use super::Hold;
use crate::types::{Caps, Type, Var};

impl Checker<'_> {
    /// At the end of a declaration: the least capture rows, then each
    /// obligation, `Outlives` and (Err-Raise).
    pub(in crate::check) fn solve_regions(&mut self) -> Check<()> {
        loop {
            let mut grown = false;
            for i in 0..self.regions.holds.len() {
                let h = self.regions.holds[i].clone();
                let (missing, room) = self.excess(&h);
                if missing.is_empty() {
                    continue;
                }
                if let Some(m) = room {
                    let level = self.slots[m as usize].level;
                    let tail = self.meta();
                    self.slots[tail as usize].level = level;
                    let mut grow = missing;
                    grow.vars.insert(Var::Meta(tail));
                    self.bind_caps(m, grow)?;
                    grown = true;
                }
            }
            if grown {
                continue;
            }
            // nothing more grows: what still waits is the least, `∅`
            let mut closed = false;
            for h in self.regions.holds.clone() {
                let mut all = Caps::default();
                for t in h.held.iter().chain(&h.within) {
                    all = all.union(&self.reach(t));
                }
                all = all.union(&self.zonk_caps(&h.allowed));
                for v in all.vars {
                    if let Var::Meta(m) = v
                        && self.slots[m as usize].solved.is_none()
                    {
                        self.bind_caps(m, Caps::default())?;
                        closed = true;
                    }
                }
            }
            if !closed {
                break;
            }
        }
        for h in self.regions.holds.clone() {
            let (missing, _) = self.excess(&h);
            if !missing.atoms.is_empty() {
                return error(
                    h.rule,
                    format!(
                        "this value holds {missing}, which `{}` does not allow (`FLICK-G001`)",
                        h.within
                            .iter()
                            .map(|t| self.zonk(t).to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            }
        }
        for (c, r, facts) in self.regions.needs.clone() {
            self.outlives(&c, &r, &facts)?;
        }
        for (e, open) in self.regions.raised.clone() {
            for args in self.zonk_row(&e).elems.values() {
                for a in args.iter() {
                    if let Some(s) = self.reach(a).atoms.iter().find_map(|v| match v {
                        Var::Named(n) if open.contains(n) => Some(n.clone()),
                        _ => None,
                    }) {
                        return error(
                            "Err-Raise",
                            format!("an error carries region `{s}` (`FLICK-E005`)"),
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// What `h` holds beyond what it allows, and the capture row that may
    /// grow to allow it. A type never inferred holds nothing (Р31).
    fn excess(&self, h: &Hold) -> (Caps, Option<u32>) {
        let mut held = Caps::default();
        for t in &h.held {
            held = held.union(&self.reach(t));
        }
        let mut allowed = self.zonk_caps(&h.allowed);
        for t in &h.within {
            allowed = allowed.union(&self.reach(t));
        }

        // shortcut: an obligation compares regions, the atoms T4 is about,
        // not the type variables and capture variables of generic code (Р62:
        // how a bound `α^{≤C}` and the atom `α` relate is not written); a
        // generic function that hides a type variable passes. Compare them
        // once Р62 is decided. A capture variable of a signature still
        // grows a row, for `Outlives`
        let mut missing = Caps::default();
        for a in &held.atoms {
            if !allowed.atoms.contains(a) && self.is_region(a) {
                missing.atoms.insert(a.clone());
            }
        }
        for v in &held.vars {
            if !allowed.vars.contains(v) && matches!(v, Var::Named(_)) {
                missing.vars.insert(v.clone());
            }
        }
        let room = allowed.vars.iter().find_map(|v| match v {
            Var::Meta(m) if self.slots[*m as usize].solved.is_none() => Some(*m),
            _ => None,
        });
        (missing, room)
    }

    /// `Θ ⊢ Outlives C ϱ` (§2.4).
    fn outlives(&self, c: &Caps, r: &Type, facts: &[(Caps, Type)]) -> Check<()> {
        let c = self.zonk_caps(c);
        let Type::Var(r) = self.zonk(r) else {
            return error("Outlives", format!("`{}` is no region", self.zonk(r)));
        };
        for k in &c.atoms {
            let unknown = matches!(k, Var::Meta(m) if self.slots[*m as usize].solved.is_none())
                && !self.is_region(k);
            if !unknown && !self.longer(k, &r, facts, 0) {
                return error(
                    "Outlives",
                    format!("`{k}` may end before region `{r}` (`FLICK-G003`)"),
                );
            }
        }
        // a capture row nothing grew is the least, `∅`
        for v in c.vars.iter().filter(|v| matches!(v, Var::Named(_))) {
            let given = facts.iter().any(|(fc, fr)| {
                let fc = self.zonk_caps(fc);
                matches!(self.zonk(fr), Type::Var(fr) if fc.vars.contains(v) && self.longer(&fr, &r, facts, 0))
            });
            if !given {
                return error(
                    "Outlives",
                    format!("the captures `{v}` are not known to outlive `{r}` (`FLICK-G003`)"),
                );
            }
        }
        Ok(())
    }

    /// `Θ ⊢ k ≥ ϱ`.
    fn longer(&self, k: &Var, r: &Var, facts: &[(Caps, Type)], depth: usize) -> bool {
        if k == r {
            return true;
        }
        let scope = |v: &Var| match v {
            Var::Named(n) => self.regions.scopes.get(n).map(|_| n.clone()),
            Var::Meta(_) => None,
        };
        if let Some(inner) = scope(r) {
            match scope(k) {
                // a region of the signature or a type variable outlives a
                // `scope` of the body; a `scope` the ones inside it
                None => return true,
                Some(outer) => {
                    let mut at = Some(inner);
                    while let Some(s) = at {
                        if s == outer {
                            return true;
                        }
                        at = self.regions.scopes.get(&s).and_then(|(_, p)| p.clone());
                    }
                }
            }
        }
        depth < 8
            && facts.iter().any(|(fc, fr)| {
                matches!(self.zonk(fr), Type::Var(fr) if self.zonk_caps(fc).atoms.contains(k) && self.longer(&fr, r, facts, depth + 1))
            })
    }
}
