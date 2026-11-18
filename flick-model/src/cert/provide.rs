//! Services in a certificate: an operation `(ask_S ()).o` is the core's
//! `perform`, and `provide L e` the handler it lowers to (`docs/formal/core.md`
//! §2): `let w = ⟦L⟧ in hdl S (o x → w.o x) (λv. v) ⟦e⟧`, the arm reading the
//! layer's record through `w`. `Layer.effect S task` builds it once, before
//! the body: `⟦L⟧ = await ⟦task⟧`.

use std::iter;
use std::rc::Rc;

use crate::check::{Inst, substitute};
use crate::syntax::{Const, Head, Name, Term};
use crate::translate::lean::elide::{row, stage_pred};
use crate::types::{Fields, Row, Stage, Type};

use super::Unsupported;
use super::sig::{Op, Sig, bare};
use super::term::{Cx, tm};

/// The service and operation of `(ask_S ()).o`.
pub(super) fn operation(e: &Term) -> Option<(String, &Name)> {
    let Term::Field(r, o) = e else { return None };
    let Term::App(ask, _) = &**r else { return None };
    match &**ask {
        Term::Apply(Head::Const(Const::Ask(s)), xs) if xs.is_empty() => Some((s.to_string(), o)),
        _ => None,
    }
}

/// The service and implementation of a layer, and whether a task builds it.
fn layer(l: &Term) -> Result<(String, &Term, bool), Unsupported> {
    let Term::App(f, x) = l else {
        return Err("a layer of no `succeed` or `effect`".into());
    };
    match &**f {
        Term::Apply(Head::Const(Const::LayerSucceed(s)), xs) if xs.is_empty() => {
            Ok((s.to_string(), x, false))
        }
        Term::Apply(Head::Const(Const::LayerEffect(s)), xs) if xs.is_empty() => {
            Ok((s.to_string(), x, true))
        }
        _ => Err("a layer of no `succeed` or `effect`".into()),
    }
}

/// The one operation of a service.
fn single<'s>(sig: &'s Sig, svc: &str) -> Result<&'s Op, Unsupported> {
    match sig.ops.get(svc).map(Vec::as_slice) {
        Some([op]) => Ok(op),
        Some(_) => Err("a service with several operations".into()),
        None => Err(format!("the service `{svc}`, not in the signature")),
    }
}

/// `provide L e` in the core.
pub(super) fn provide_tm(
    l: &Term,
    body: &Term,
    ctx: &[Name],
    sig: &Sig,
) -> Result<String, Unsupported> {
    let (svc, imp, effect) = layer(l)?;
    let op = single(sig, &svc)?;
    let w = tm(imp, ctx, sig)?;
    let w = if effect { format!("(.await {w})") } else { w };
    let inner: Vec<Name> = iter::once(Name::from("$w"))
        .chain(ctx.iter().cloned())
        .collect();
    Ok(format!(
        "(.let_ {w} (.hdl {svc:?} (recArm {:?}) (.lam (.var 0)) {}))",
        op.name.to_string(),
        tm(body, &inner, sig)?
    ))
}

impl Cx<'_> {
    /// The instance `S ū` a constant of a service took at `head` (`ask`,
    /// `Layer.succeed`, `Layer.effect`): `ū`, printed, and the type of the
    /// operation `op` at it, `a[ū] ->{} b[ū]`.
    fn at_instance(&self, head: &Term, op: &Op) -> Result<(Vec<Type>, String, Type), Unsupported> {
        let insts: &[Inst] = (self.node(head)?.instance.as_ref()).map_or(&[], |(_, i)| i);
        let insts =
            (insts.get(..op.vars.len())).ok_or("a service's constant without its instance")?;
        let args = (insts.iter())
            .map(|i| match i {
                Inst::Type(t) => Ok(t.clone()),
                _ => Err("a service at an argument of no type".to_owned()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let us = (insts.iter())
            .map(|i| self.inst(i))
            .collect::<Result<Vec<_>, _>>()?;
        let map = op.vars.iter().cloned().zip(insts.iter().cloned()).collect();
        Ok((
            args,
            format!("[{}]", us.join(", ")),
            substitute(&op.ty, &map),
        ))
    }

    /// (Perform) of an operation called, at its arrow `a[ū] ->{S ū} b[ū]`:
    /// the model types it `a[ū] ->{} b[ū]` and asks for `S ū` where it runs.
    pub(super) fn operation(&self, e: &Term) -> Result<(String, Type), Unsupported> {
        let (svc, o) = operation(e).ok_or("no operation")?;
        let op = (single(self.sig, &svc)?.name == *o)
            .then(|| single(self.sig, &svc))
            .ok_or("an operation not in the signature")??;
        let Term::Field(r, _) = e else {
            return Err("no operation".into());
        };
        let Term::App(ask, _) = &**r else {
            return Err("no operation".into());
        };
        let Term::Apply(Head::Const(Const::Ask(path)), _) = &**ask else {
            return Err("no operation".into());
        };
        let (args, us, at_op) = self.at_instance(ask, op)?;
        let Type::Fun(a, _, b) = &at_op else {
            return Err("an operation of no arrow".into());
        };
        let mut wants = Row::none();
        wants.elems.insert(path.clone(), args.into());
        let at = Type::Fun(a.clone(), Stage::Runtime(wants, Row::none()), b.clone());
        let proof = format!(
            "(Cert.typ_op (E := {svc:?}) (o := {:?}) (us := {us}) (a' := {}) (b' := {}) (hS.arity_of (by decide) rfl) ((hS.op _ _ (by decide)).trans rfl) (by inst_eq) (by inst_eq) (by le_live))",
            o.to_string(),
            self.ty(a)?,
            self.ty(b)?
        );
        Ok((proof, at))
    }

    /// `provide` by `Cert.typ_provide`: the record at the type of the one
    /// operation, the body on its own stage under `w`.
    pub(super) fn provide(
        &mut self,
        l: &Term,
        body: &Term,
        t: &Type,
        col: usize,
    ) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        let (svc, imp, effect) = layer(l)?;
        let op = single(self.sig, &svc)?;
        let Term::App(head, _) = l else {
            return Err("a layer of no `succeed` or `effect`".into());
        };
        let (_, us, at_op) = self.at_instance(head, op)?;
        let Type::Fun(a, _, b) = &at_op else {
            return Err("an operation of no arrow".into());
        };
        let record = Type::Record(Fields {
            fields: [(op.name.clone(), at_op.clone())].into(),
            tail: None,
        });
        // the row outside fits the body: at once when the service has no parameters
        let fits = if self.sig.arity.get(&svc).copied().unwrap_or(0) == 0 {
            "(Cert.body_none ((hS.arity _ (by decide)).trans rfl))"
        } else {
            "(by body_row)"
        };
        let (own, rest) = (self.node(body)?.own.clone()).ok_or("a `provide` without its stage")?;
        // the rows outside as row terms: the handler's value and arms are typed at them
        let errors = match &own {
            Stage::Pure => ".cnil".to_owned(),
            Stage::Runtime(_, e) => row(e, &self.env)?,
        };
        let w = if effect {
            // the layer's task is checked against its parameter, not inferred
            let node = self.node(imp)?;
            let built = (node.inferred.clone())
                .or_else(|| node.expected.clone())
                .ok_or("a layer's task without its type")?;
            let Type::Eff(r, e, _) = bare(&built).clone() else {
                return Err("a layer built of no task".into());
            };
            self.await_(imp, &Type::Eff(r, e, Rc::new(record.clone())), col + 2)?
        } else {
            self.typing(imp, &record, col + 2)?
        };
        let pb = self.under(&"$w".into(), |c| c.typing(body, t, col + 2))?;
        Ok(format!(
            "(Cert.typ_provide (E := {svc:?}) (o := {:?}) (us := {us}) (a' := {}) (b' := {}) (R := {}) (t := {}) (r := {}) (ε := {errors}) (φ₀ := {}){pad}(hS.arity_of (by decide) rfl) ((hS.op _ _ (by decide)).trans rfl) (hS.complete rfl) (by inst_eq) (by inst_eq) rfl{pad}{w}{pad}{pb}{pad}(by stage_le) {fits} (by ob_out) (by stage_le) (by le_live))",
            op.name.to_string(),
            self.ty(a)?,
            self.ty(b)?,
            self.ty(&record)?,
            self.ty(t)?,
            row(&rest, &self.env)?,
            stage_pred(&own, &self.env)?
        ))
    }
}
