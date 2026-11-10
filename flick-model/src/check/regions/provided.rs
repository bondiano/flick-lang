//! `Fiber.Provided R ϱ` (`docs/formal/regions.md` §2.7, `docs/formal/runtime.md#a-provide-service-outlives-the-release-of-its-layer`): work an
//! owner keeps runs with the services of the place that registered it, so
//! each service of `R` comes from a provider that outlives `ϱ`.
//!
//! The constraint stands as `{Provided R ϱ}` in the row of the last
//! runtime arrow of its scheme (`Type::with_provided`). The call splits it
//! into `{Provided {S} ϱ}`, one per service, and they go up the stage with
//! its row: the innermost `provide` that builds `S` settles one if its
//! region outlives `ϱ`, else `FLICK-G003`; the `scope` of `ϱ` settles it;
//! at the end of a λ or a task a given `Provided R ϱ` with `S ∈ R` does.
//! What is left goes with the row of the λ or the task to what runs it,
//! and a signature's row has no such element: `FLICK-G003` there too.

use std::rc::Rc;

use super::super::{Check, Checker, error};
use super::{live_service, scope_service};
use crate::syntax::{Name, Path};
use crate::types::{PROVIDED, Row, Stage, Type, Var, provided};

/// The key of `{Provided {S} ϱ}`: the service, or `~ρ` for a rigid tail.
fn key(service: &str) -> Path {
    Path::new(PROVIDED, service)
}

impl Checker<'_> {
    /// The call splits `{Provided R ϱ}` of the row it performs by the
    /// services of `R`, but `Scope` and the obligations; an unsolved tail
    /// of `R` is the least, none.
    pub(in crate::check) fn split_provided(&mut self, r: &Row) -> Check<Row> {
        let mut r = self.zonk_row(r);
        let Some(args) = r.elems.remove(&provided()) else {
            return Ok(r);
        };
        let [Type::Tag(wanted), owner] = &args[..] else {
            return error("Provided", "a malformed `Fiber.Provided`");
        };
        let wanted = &self.zonk_row(wanted);
        self.keep_live(wanted, owner);
        let services = wanted
            .elems
            .keys()
            .filter(|s| {
                **s != scope_service()
                    && **s != live_service()
                    && **s != provided()
                    && &*s.module != PROVIDED
            })
            .map(ToString::to_string);
        let tails = wanted.vars.iter().filter_map(|v| match v {
            Var::Named(n) => Some(format!("~{n}")),
            Var::Meta(_) => None,
        });
        for s in services.chain(tails).collect::<Vec<_>>() {
            match r.elems.get(&key(&s)).cloned() {
                // one element a service: two owners are one region
                Some(old) => self.unify(&old[0], owner)?,
                None => {
                    r.elems.insert(key(&s), Rc::from([owner.clone()]));
                }
            }
        }
        Ok(r)
    }

    /// The `scope` of region `s` settles what is kept until `s` closes:
    /// the services come from what runs the stage, which outlives `s`.
    pub(in crate::check) fn settle_scope(&self, r: &mut Row, s: &Name) {
        let region = Type::Var(Var::Named(s.clone()));
        r.elems
            .retain(|k, a| &*k.module != PROVIDED || self.zonk(&a[0]) != region);
    }

    /// The `provide` of region `p` building `outputs` settles what uses
    /// them if `p` outlives the owner, else `FLICK-G003`. Without `p`, the
    /// `with θ` of a configuration: what it holds is built already.
    pub(in crate::check) fn settle_provide(
        &mut self,
        r: &mut Row,
        outputs: &Row,
        p: Option<&Name>,
    ) -> Check<()> {
        let built: Vec<Path> = r
            .elems
            .keys()
            .filter(|k| &*k.module == PROVIDED && outputs.elems.contains_key(&Path::parse(&k.name)))
            .cloned()
            .collect();
        for k in built {
            let owner = self.zonk(&r.elems[&k][0]);
            match (&owner, p) {
                // shortcut: an owner not known yet passes on, flick-core decides it at the end of the declaration; wait for it once a generated owner is a meta here
                (Type::Var(Var::Meta(_)), Some(_)) => continue,
                (Type::Var(Var::Named(n)), Some(p)) if !self.encloses(p, n) => {
                    return error(
                        "Provided",
                        format!(
                            "work kept until `{n}` closes uses `{}`, which its `provide` releases earlier (`FLICK-G003`)",
                            k.name
                        ),
                    );
                }
                _ => {}
            }
            r.elems.remove(&k);
        }
        Ok(())
    }

    /// Whether `inner` is `outer` or nested in it (§2.4).
    fn encloses(&self, outer: &Name, inner: &Name) -> bool {
        let mut at = Some(inner.clone());
        while let Some(s) = at {
            if s == *outer {
                return true;
            }
            at = self.regions.scopes.get(&s).and_then(|(_, p)| p.clone());
        }
        false
    }

    /// The end of a λ or a task: a given `Provided R ϱ` settles `{Provided
    /// {S} ϱ}` with `S ∈ R`.
    // shortcut: a given settles at the end of the λ or task around the call, not at the call (§2.7), so a λ with a signature row between them rejects what §2.7 accepts; settle at the call once the generator makes such a λ
    pub(in crate::check) fn settle_given(&self, r: &mut Row) {
        r.elems.retain(|k, a| {
            if &*k.module != PROVIDED {
                return true;
            }
            let owner = self.zonk(&a[0]);
            !self.regions.givens.iter().any(|(g, s)| {
                let g = self.zonk_row(g);
                let named = match k.name.strip_prefix('~') {
                    Some(v) => g.vars.contains(&Var::Named(v.into())),
                    None => g.elems.contains_key(&Path::parse(&k.name)),
                };
                named && self.zonk(s) == owner
            })
        });
    }

    /// The `Provided R ϱ` a signature `t` gives its body.
    pub(in crate::check) fn give_provided(&mut self, t: &Type) {
        match t {
            Type::Fun(_, s, r) => {
                if let Stage::Runtime(row, _) = s
                    && let Some(args) = row.elems.get(&provided())
                    && let [Type::Tag(g), owner] = &args[..]
                {
                    self.regions.givens.push((g.clone(), owner.clone()));
                }
                self.give_provided(r);
            }
            Type::Captured(t, _) => self.give_provided(t),
            _ => {}
        }
    }

    /// A `provide`: a region for the order of §2.4, inside the ones around.
    pub(in crate::check) fn enter_provide(&mut self) -> Name {
        self.rigid += 1;
        let p: Name = format!("provide'{}", self.rigid).into();
        let parent = self.regions.nest.last().cloned();
        self.regions.scopes.insert(p.clone(), (self.level, parent));
        self.regions.nest.push(p.clone());
        p
    }

    pub(in crate::check) fn leave_provide(&mut self) {
        self.regions.nest.pop();
    }

    pub(in crate::check) fn givens(&self) -> usize {
        self.regions.givens.len()
    }

    pub(in crate::check) fn forget_givens(&mut self, from: usize) {
        self.regions.givens.truncate(from);
    }
}
