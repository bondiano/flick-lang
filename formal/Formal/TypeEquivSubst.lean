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

theorem Row.substT_equiv {σ σ' : Atom → Ty} (h : ∀ a, Ty.Equiv (σ a) (σ' a)) (r : Row) :
    r.substT σ = r.substT σ' := by
  have hr : ∀ b, (σ b).reach = (σ' b).reach := fun b => (h b).reach
  have hs : ∀ b, (σ b).asRow = (σ' b).asRow := fun b => (h b).asRow
  funext x; cases x <;> simp only [Row.substT, hr, hs]

theorem Stage.substT_equiv {σ σ' : Atom → Ty} (h : ∀ a, Ty.Equiv (σ a) (σ' a)) (φ : Stage) :
    φ.substT σ = φ.substT σ' := by
  cases φ <;> simp only [Stage.substT, Row.substT_equiv h]

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
  | row r => simp only [Ty.substT, Row.substT_equiv h r]; exact .refl
  | cnil | pure | base | adt | rnil | bare => exact .refl
  | arrow a φ b C iha ihb =>
    simp only [Ty.substT, Stage.substT_equiv h φ, Caps.substT_equiv h C]
    exact .arrow (iha h) (ihb h)
  | ref a ih => exact .ref (ih h)
  | tapp f a ihf iha => exact .tapp (ihf h) (iha h)
  | rcons l a r iha ihr => exact .rcons (iha h) (ihr h)
  | eff r e t C ih =>
    simp only [Ty.substT, Row.substT_equiv h r, Row.substT_equiv h e, Caps.substT_equiv h C]
    exact .eff (ih h)
  | errors e => simp only [Ty.substT, Row.substT_equiv h e]; exact .refl
  | hnd a ih => exact .hnd (ih h)
  | owner a ih => exact .owner (ih h)
  | dict c a ih => exact .dict (ih h)
  | all k a ih => exact .all (ih (liftT_equiv h k))
  | rnm N a ih => exact .rnm (ih h)
  | run r s ihr ihs => exact .run (ihr h) (ihs h)
  | rless G r ih => exact .rless G (ih h)
  | rt r e ihr ihe => exact .rt (ihr h) (ihe h)

end Flick
