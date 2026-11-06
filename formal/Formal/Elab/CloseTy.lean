import Formal.Elab.T6a
import Formal.Elab.Basic

/-!
# Elaboration: the algebra of closing types (`docs/formal/elaboration.md#typing-the-whole-term-does-not-type-its-dictionaries-in-the-empty-context`)

Closing the premises of an elaboration by types `θ` moves `θ` under binders: a scheme
substitutes `lift k θ`, an instantiation commutes with it (`lift_inst`), a shift `up k` of
`letSig` commutes with it (`up_lift`). What a substituted term holds comes from what it held
(`heldS_subst`). A ground premise stays fixed by every substitution (`Cn.Ground`), and the
constants of `Γ₀` have closed schemes (`Prog.ConstsClosed`).
-/

namespace Flick.Elab

@[simp] theorem lift_zero (θ : Nat → Ty) : lift 0 θ = θ := by
  funext n; simp [lift, up_zero, Ty.subst_id]

/-- `lift j` of a composition. -/
theorem lift_comp (j : Nat) (σ τ : Nat → Ty) (n : Nat) :
    (lift j σ n).subst (lift j τ) = lift j (fun m => (σ m).subst τ) n := by
  by_cases h : n < j
  · simp [lift, h, Ty.subst]
  · simp only [lift, h, if_false, Ty.subst_comp]
    congr 1; funext m
    simp [up, Ty.subst, lift, Nat.not_lt.2 (Nat.le_add_left j m)]

/-- A shift `up k` then `lift k θ` is `θ` then the shift. -/
theorem up_lift_pt (k : Nat) (θ : Nat → Ty) :
    (fun n => (up k n).subst (lift k θ)) = fun n => (θ n).subst (up k) := by
  funext n; simp [up, Ty.subst, lift, Nat.not_lt.2 (Nat.le_add_left k n)]

theorem Ty.up_lift (k : Nat) (θ : Nat → Ty) (t : Ty) :
    (t.subst (up k)).subst (lift k θ) = (t.subst θ).subst (up k) := by
  rw [Ty.subst_comp, Ty.subst_comp, up_lift_pt]

theorem Cn.up_lift (k : Nat) (θ : Nat → Ty) (q : Cn) :
    (q.subst (up k)).subst (lift k θ) = (q.subst θ).subst (up k) := by
  rw [Cn.subst_comp, Cn.subst_comp, up_lift_pt]

theorem Ty.lift_up_lift (j k : Nat) (θ : Nat → Ty) (t : Ty) :
    (t.subst (lift j (up k))).subst (lift j (lift k θ)) =
      (t.subst (lift j θ)).subst (lift j (up k)) := by
  rw [Ty.subst_comp, Ty.subst_comp]
  congr 1; funext n
  rw [lift_comp, lift_comp, up_lift_pt]

theorem Scheme.up_lift (k : Nat) (θ : Nat → Ty) (s : Scheme) :
    (s.subst (up k)).subst (lift k θ) = (s.subst θ).subst (up k) := by
  simp only [Scheme.subst, List.map_map, Function.comp_def, Ty.lift_up_lift]
  congr 1
  refine List.map_congr_left fun q _ => ?_
  simp only [Cn.subst, List.map_map, Function.comp_def, Ty.lift_up_lift]

/-- An instantiation of a substituted scheme is the substituted instantiation. -/
theorem lift_inst_pt {k : Nat} {ts : List Ty} (hk : ts.length = k) (θ : Nat → Ty) :
    (fun n => (lift k θ n).subst (inst (ts.map (·.subst θ)))) = fun n => (inst ts n).subst θ := by
  funext n
  subst hk
  by_cases h : n < ts.length
  · simp [lift, inst, h, Ty.subst]
  · simp only [lift, h, if_false, Ty.subst_comp]
    have : (fun m => (up ts.length m).subst (inst (ts.map (·.subst θ)))) = Ty.var := by
      funext m; simp [up, Ty.subst, inst, Nat.not_lt.2 (Nat.le_add_left ts.length m)]
    rw [this, Ty.subst_id]
    simp [inst, h, Ty.subst]

theorem Ty.lift_inst {k : Nat} {ts : List Ty} (hk : ts.length = k) (θ : Nat → Ty) (t : Ty) :
    (t.subst (lift k θ)).subst (inst (ts.map (·.subst θ))) = (t.subst (inst ts)).subst θ := by
  rw [Ty.subst_comp, Ty.subst_comp, lift_inst_pt hk]

theorem Cn.lift_inst {k : Nat} {ts : List Ty} (hk : ts.length = k) (θ : Nat → Ty) (q : Cn) :
    (q.subst (lift k θ)).subst (inst (ts.map (·.subst θ))) = (q.subst (inst ts)).subst θ := by
  rw [Cn.subst_comp, Cn.subst_comp, lift_inst_pt hk]

/-- A variable of a substituted type comes from a variable of the type. -/
theorem Ty.occurs_of_subst {σ : Nat → Ty} {n : Nat} {t : Ty} (h : (t.subst σ).Occurs n) :
    ∃ m, t.Occurs m ∧ (σ m).Occurs n := by
  induction t using Ty.ind with
  | var m => exact ⟨m, rfl, h⟩
  | con | cnil => cases h
  | app _ _ ih₁ ih₂ | cun _ _ ih₁ ih₂ | run _ _ ih₁ ih₂ =>
    rcases h with h | h
    · obtain ⟨m, hm, h⟩ := ih₁ h; exact ⟨m, .inl hm, h⟩
    · obtain ⟨m, hm, h⟩ := ih₂ h; exact ⟨m, .inr hm, h⟩
  | rnm _ _ ih => exact ih h
  | arr φ _ _ _ ih₁ ih₂ ih₃ ihφ =>
    rcases h with h | h | h | h
    · obtain ⟨m, hm, h⟩ := ih₁ h; exact ⟨m, .inl hm, h⟩
    · obtain ⟨m, hm, h⟩ := ih₂ h; exact ⟨m, .inr (.inl hm), h⟩
    · obtain ⟨m, hm, h⟩ := ih₃ h; exact ⟨m, .inr (.inr (.inl hm)), h⟩
    · cases φ with
      | P => exact h.elim
      | R r e =>
        rcases h with h | h
        · obtain ⟨m, hm, h⟩ := ihφ.1 h; exact ⟨m, .inr (.inr (.inr (.inl hm))), h⟩
        · obtain ⟨m, hm, h⟩ := ihφ.2 h; exact ⟨m, .inr (.inr (.inr (.inr hm))), h⟩

theorem Ty.occurs_lift {σ : Nat → Ty} {k n : Nat} {t : Ty} (h : (t.subst (lift k σ)).Occurs (n + k)) :
    ∃ m, t.Occurs (m + k) ∧ (σ m).Occurs n := by
  obtain ⟨m', hm', h⟩ := Ty.occurs_of_subst h
  unfold lift at h
  by_cases hk : m' < k
  · simp only [hk, if_true, Ty.Occurs] at h; omega
  · simp only [hk, if_false] at h
    obtain ⟨j, hj, h⟩ := Ty.occurs_of_subst h
    simp only [up, Ty.Occurs] at h
    refine ⟨m' - k, ?_, ?_⟩
    · rwa [Nat.sub_add_cancel (by omega)]
    · rwa [show n = j by omega]

theorem Scheme.reach_subst {σ : Nat → Ty} {s : Scheme} {n : Nat} (h : (s.subst σ).reach n) :
    ∃ m, s.reach m ∧ (σ m).Occurs n := by
  rcases h with h | ⟨hc, h⟩
  · obtain ⟨m, hm, h⟩ := Ty.occurs_lift h; exact ⟨m, .inl hm, h⟩
  · obtain ⟨m, hm, h⟩ := Ty.occurs_lift h
    exact ⟨m, .inr ⟨fun e => hc (by simp [Scheme.subst, e]), hm⟩, h⟩

/-- What a term holds in a substituted context. -/
theorem heldS_subst {σ : Nat → Ty} {Γ : List Scheme} {e : Exp} {n : Nat}
    (h : heldS (Γ.map (·.subst σ)) e n) : ∃ m, heldS Γ e m ∧ (σ m).Occurs n := by
  obtain ⟨x, s, hx, hs, hr⟩ := h
  rw [List.getElem?_map] at hs
  cases hΓ : Γ[x]? with
  | none => rw [hΓ] at hs; cases hs
  | some s₀ =>
    rw [hΓ] at hs; cases hs
    obtain ⟨m, hm, h⟩ := Scheme.reach_subst hr
    exact ⟨m, ⟨x, s₀, hx, hΓ, hm⟩, h⟩

theorem Ty.subst_ground {t : Ty} (h : t.Ground) (σ : Nat → Ty) : t.subst σ = t := by
  rw [Ty.subst_agree (σ' := .var) fun n hn => (h n hn).elim, Ty.subst_id]

theorem Cn.subst_ground {q : Cn} (h : q.Ground) (σ : Nat → Ty) : q.subst σ = q := by
  cases q with
  | mk c args =>
    simp only [Cn.subst, Cn.mk.injEq, true_and]
    conv => rhs; rw [← List.map_id args]
    exact List.map_congr_left fun t ht => Ty.subst_ground (h t ht) σ

/-- The constants of `Γ₀` have closed schemes: their types mention only their binders. -/
def Prog.ConstsClosed (P : Prog) : Prop :=
  ∀ c s, P.consts c = some s → ∀ σ σ' : Nat → Ty, (∀ j, j < s.k → σ j = σ' j) →
    (dicts .cnil s.ctx (.mono s.ty)).subst σ = (dicts .cnil s.ctx (.mono s.ty)).subst σ'

/-- An instantiation of a closed scheme, substituted, is an instantiation. -/
theorem const_inst {P : Prog} (hP : P.ConstsClosed) {c : Nat} {s : Scheme} (hc : P.consts c = some s)
    {ts : List Ty} (hk : ts.length = s.k) (θ : Nat → Ty) :
    (dicts .cnil s.ctx (.mono s.ty)).subst (inst (ts.map (·.subst θ))) =
      dicts .cnil ((s.ctx.map (·.subst (inst ts))).map (·.subst θ))
        (.mono ((s.ty.subst (inst ts)).subst θ)) := by
  rw [hP c s hc _ (fun n => (inst ts n).subst θ) fun j hj => by
    simp [inst, hk, hj]]
  simp only [TTy.subst_dicts, TTy.subst, List.map_map, Function.comp_def, Cn.subst_comp,
    Ty.subst_comp]
  rfl

/-- The constructors have closed types: they mention only their variables. -/
def Prog.CtorsClosed (P : Prog) : Prop :=
  ∀ K c, P.ctors K = some c → ∀ σ σ' : Nat → Ty, (∀ j, j < c.k → σ j = σ' j) →
    (dicts .cnil c.ctx (.mono c.ty)).subst σ = (dicts .cnil c.ctx (.mono c.ty)).subst σ'

/-- An instantiation of a constructor, substituted, is an instantiation. -/
theorem ctor_inst {P : Prog} (hP : P.CtorsClosed) {K : Nat} {c : Ctor} (hK : P.ctors K = some c)
    {ts : List Ty} (hk : ts.length = c.k) (θ : Nat → Ty) :
    (dicts .cnil c.ctx (.mono c.ty)).subst (inst (ts.map (·.subst θ))) =
      dicts .cnil ((c.ctx.map (·.subst (inst ts))).map (·.subst θ))
        (.mono ((c.ty.subst (inst ts)).subst θ)) := by
  rw [hP K c hK _ (fun n => (inst ts n).subst θ) fun j hj => by
    simp [inst, hk, hj]]
  simp only [TTy.subst_dicts, TTy.subst, List.map_map, Function.comp_def, Cn.subst_comp,
    Ty.subst_comp]
  rfl

end Flick.Elab
