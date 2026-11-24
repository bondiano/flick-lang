import Formal.TypeEquiv

/-!
# Substitution by equivalent types

An invariant argument may be replaced by an equivalent row term. The same
replacement must remain valid under every type constructor and binder.
-/

namespace Flick

theorem Caps.substT_equiv {σ σ' : Atom → Ty} (h : ∀ a, Ty.Equiv (σ a) (σ' a)) (C : Caps) :
    C.substT σ = C.substT σ' := by
  have hr : ∀ b, (σ b).reach = (σ' b).reach := fun b => (h b).reach
  funext a; simp only [Caps.substT, hr]

theorem liftT_equiv {σ σ' : Atom → Ty} (h : ∀ a, Ty.Equiv (σ a) (σ' a)) (k : Nat) (a : Atom) :
    Ty.Equiv (liftT k σ a) (liftT k σ' a) := by
  cases a with
  | v i =>
    by_cases hi : i < k
    · simp only [liftT, hi, if_true]; exact .refl
    · simp only [liftT, hi, if_false]; exact (h _).up k
  | r p => exact (h _).up k

theorem Ty.substT_equiv (t : Ty) {σ σ' : Atom → Ty} (h : ∀ a, Ty.Equiv (σ a) (σ' a)) :
    Ty.Equiv (t.substT σ) (t.substT σ') := by
  induction t generalizing σ σ' with
  | tvar n => exact h (.v n)
  | rg p => exact h (.r p)
  | caps C => simp only [Ty.substT, Caps.substT_equiv h C]; exact .refl
  | cnil | pure | base | adt | rnil | bare => exact .refl
  | arrow a φ b C iha ihφ ihb =>
    simp only [Ty.substT_arrow, Caps.substT_equiv h C]
    refine .arrow (iha h) ?_ (ihb h)
    unfold Ty.substS; split
    · exact ihφ h
    · exact .refl
  | ref a ih => exact .ref (ih h)
  | funcRef a ih => exact .funcRef (ih h)
  | tapp f a ihf iha => exact .tapp (ihf h) (iha h)
  | rcons l a r iha ihr => exact .rcons (iha h) (ihr h)
  | eff r e t C ihr ihe iht =>
    simp only [Ty.substT, Caps.substT_equiv h C]
    exact .eff (ihr h) (ihe h) (iht h)
  | lazy r e t C ihr ihe iht =>
    simp only [Ty.substT, Caps.substT_equiv h C]
    exact .lazy (ihr h) (ihe h) (iht h)
  | lazyCell r e t C ihr ihe iht =>
    simp only [Ty.substT, Caps.substT_equiv h C]
    exact .lazyCell (ihr h) (ihe h) (iht h)
  | errors e ih => exact .errors (ih h)
  | hnd a ih => exact .hnd (ih h)
  | owner a ih => exact .owner (ih h)
  | dict c a ih => exact .dict (ih h)
  | all k a ih => exact .all (ih (liftT_equiv h k))
  | rnm N a ih => exact .rnm (ih h)
  | run r s ihr ihs => exact .run (ihr h) (ihs h)
  | rless G r ih => exact .rless G (ih h)
  | rt r e ihr ihe => exact .rt (ihr h) (ihe h)
  | rtob r s e ihr ihs ihe => exact .rtob (ihr h) (ihs h) (ihe h)

theorem TyClass.substT_equiv {σ σ' : Atom → Ty} (h : ∀ a, Ty.Equiv (σ a) (σ' a)) (c : TyClass) :
    c.substT σ = c.substT σ' := by
  induction c using Quotient.ind with
  | _ t => exact (t.substT_equiv h).classOf

theorem Row.substT_equiv {σ σ' : Atom → Ty} (h : ∀ a, Ty.Equiv (σ a) (σ' a)) (r : Row) :
    r.substT σ = r.substT σ' := by
  have hr : ∀ b, (σ b).reach = (σ' b).reach := fun b => (h b).reach
  have hs : ∀ b, (σ b).asRow = (σ' b).asRow := fun b => (h b).asRow
  funext x; cases x <;> simp only [Row.substT, hr, hs, TyClass.substT_equiv h]

theorem Stage.substT_equiv {σ σ' : Atom → Ty} (h : ∀ a, Ty.Equiv (σ a) (σ' a)) (φ : Stage) :
    φ.substT σ = φ.substT σ' := by
  cases φ <;> simp only [Stage.substT, Row.substT_equiv h]

end Flick
