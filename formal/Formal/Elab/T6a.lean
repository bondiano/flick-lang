import Formal.Elab.Target

/-!
# Elaboration: T6(a) (elaboration.md §4.2)

`ent_typed`: a derived dictionary has its dictionary type, `Δ ⊩ Q ⇝ d ⇒ ⟦Δ⟧ ⊢⁺ d : D_Q`.
`T6a`: an elaboration has the translated type, by induction on `Elab`.
-/

namespace Flick.Elab

variable {P : Prog} (X : Target P)

theorem TTy.subst_dicts (σ : Nat → Ty) (qs : List Cn) (t : TTy) :
    (dicts qs t).subst σ = dicts (qs.map (·.subst σ)) (t.subst σ) := by
  induction qs with
  | nil => rfl
  | cons q qs ih => simp only [dicts, List.foldr_cons, List.map_cons] at ih ⊢; simp [TTy.subst, ih]

theorem Scheme.tr_subst (σ : Nat → Ty) (s : Scheme) : (s.subst σ).tr = s.tr.subst σ := by
  simp [Scheme.tr, Scheme.subst, TSch.subst, TTy.subst_dicts, TTy.subst]

theorem lift_up_zero (k : Nat) : lift k (up 0) = .var := by
  funext n
  simp only [lift, up, Nat.add_zero, Ty.subst]
  split <;> first | rfl | (congr 1; omega)

theorem Ty.subst_id (t : Ty) : t.subst .var = t := by
  induction t with
  | var n => rfl
  | con c => rfl
  | app t u iht ihu => simp [Ty.subst, iht, ihu]

theorem TTy.subst_id (t : TTy) : t.subst .var = t := by
  induction t with
  | mono t => simp [TTy.subst, Ty.subst_id]
  | arr φ a b iha ihb => simp [TTy.subst, iha, ihb]
  | dict q => simp [TTy.subst, Cn.subst, Ty.subst_id]

theorem up_zero : up 0 = .var := by funext n; simp [up]

theorem TSch.subst_up_zero (s : TSch) : s.subst (up 0) = s := by
  simp [TSch.subst, lift_up_zero, TTy.subst_id]

theorem Cn.subst_up_zero (q : Cn) : q.subst (up 0) = q := by
  simp [Cn.subst, up_zero, Ty.subst_id]

/-- The dictionaries `d̄` have the types `D_Q̄`. -/
abbrev DTy (Γ : List TSch) (Δ : List Cn) (φ : Stage) : List Dict → List Cn → Prop :=
  All2 fun d q => X.Has Γ Δ (.dict d) (.dict q) φ

theorem dict_apps {Γ Δ φ f ds qs t} (hf : X.Has Γ Δ (.dict f) (dicts qs t) φ)
    (h : DTy X Γ Δ φ ds qs) : X.Has Γ Δ (.dict (apps f ds)) t φ := by
  induction h generalizing f with
  | nil => exact hf
  | cons hd _ ih => exact ih (X.dap hf hd)

theorem tm_dapps {Γ Δ φ f ds qs t} (hf : X.Has Γ Δ f (dicts qs t) φ)
    (h : DTy X Γ Δ φ ds qs) : X.Has Γ Δ (f.dapps ds) t φ := by
  induction h generalizing f with
  | nil => exact hf
  | cons hd _ ih => exact ih (X.dapp hf hd)

theorem clo_typed {Γ Δ φ d Q} (h : Clo P Δ d Q) : X.Has Γ Δ (.dict d) (.dict Q) φ := by
  induction h with
  | given hk => exact X.dvar hk
  | super _ hs ih => exact X.slot ih hs

/-- **Dictionaries are typed**: `Δ ⊩ Q̄ ⇝ d̄ ⇒ ⟦Δ⟧ ⊢⁺ d̄ : D_Q̄`. -/
theorem ent_typed {Γ Δ φ qs ds} (h : Ent P Δ qs ds) : DTy X Γ Δ φ ds qs := by
  induction h with
  | nil => exact .nil
  | cons _ _ ih₁ ih₂ => cases ih₁ with | cons r _ => exact .cons r ih₂
  | given hc => exact .cons (clo_typed X hc) .nil
  | prim h₁ h₂ => exact .cons (X.prim h₁ h₂) .nil
  | strct h₁ h₂ _ ih => exact .cons (dict_apps X (X.strct h₁ h₂) ih) .nil
  | @inst Q I ins σ ds h₁ h₂ h₃ _ ih =>
    have hq : Q = ⟨ins.cls, ins.head.map (·.subst σ)⟩ := by cases Q; simp_all
    subst hq
    exact .cons (dict_apps X (X.inst h₁) ih) .nil

/-- **T6(a) (types are preserved).** `Γ; Δ ⊢ e : τ ! φ ⇝ e'` gives
`⟦Γ⟧, ⟦Δ⟧ ⊢⁺ e' : τ ! φ` in any target. -/
theorem T6a {Γ Δ e τ φ e'} (h : Elab P Γ Δ e τ φ e') :
    X.Has (Γ.map Scheme.tr) Δ e' (.mono τ) φ := by
  induction h with
  | @var Γ Δ x σ ts ds φ hx hk he =>
    have hv := X.var (Γ := Γ.map Scheme.tr) (Δ := Δ) (φ := φ) (ts := ts)
      (by rw [List.getElem?_map, hx]; rfl) hk
    simp only [Scheme.tr, TTy.subst_dicts, TTy.subst] at hv
    exact tm_dapps X hv (ent_typed X he)
  | meth hm hc _ _ h₀ he =>
    cases ent_typed X h₀ with
    | cons r _ => exact tm_dapps X (X.meth hm hc r) (ent_typed X he)
  | app _ _ ih₁ ih₂ => exact X.app ih₁ ih₂
  | lam _ ih => exact X.lam ih
  | let_ _ _ ih₁ ih₂ =>
    refine X.let_ (k := 0) ?_ (fun h => absurd rfl h) (.inr rfl) ih₂
    simpa [TSch.subst_up_zero, Cn.subst_up_zero] using ih₁
  | @letSig Γ Δ σ e b t φ₁ φ e' b' _ hp hφ _ ih₁ ih₂ =>
    have hd := X.lamD ih₁ fun h => hp (.inl h)
    rw [List.map_map] at hd
    simp only [Function.comp_def, Scheme.tr_subst] at hd
    have hΓ : Γ.map (fun x => x.tr.subst (up σ.k)) = (Γ.map Scheme.tr).map (·.subst (up σ.k)) := by
      simp
    rw [hΓ] at hd
    exact X.let_ hd (fun h => hp (.inr h)) hφ ih₂

end Flick.Elab
