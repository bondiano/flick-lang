import Formal.Elab.Hyp

/-! Substitutions, application spines and inversion of entailment. -/

namespace Flick.Elab

theorem Ty.subst_comp (t : Ty) (σ θ : Nat → Ty) :
    (t.subst σ).subst θ = t.subst fun n => (σ n).subst θ := by
  induction t <;> simp_all [Ty.subst]

theorem Cn.subst_comp (c : Cn) (σ θ : Nat → Ty) :
    (c.subst σ).subst θ = c.subst fun n => (σ n).subst θ := by
  simp [Cn.subst, Ty.subst_comp]

theorem Ty.subst_agree {t : Ty} {σ σ' : Nat → Ty} (h : ∀ n, t.Occurs n → σ n = σ' n) :
    t.subst σ = t.subst σ' := by
  induction t with
  | var n => exact h n rfl
  | con c => rfl
  | cnil => rfl
  | app t u iht ihu =>
    simp only [Ty.subst]
    rw [iht fun n hn => h n (.inl hn), ihu fun n hn => h n (.inr hn)]
  | cun t u iht ihu =>
    simp only [Ty.subst]
    rw [iht fun n hn => h n (.inl hn), ihu fun n hn => h n (.inr hn)]
  | arr φ a b q iha ihb ihq =>
    simp only [Ty.subst]
    rw [iha fun n hn => h n (.inl hn), ihb fun n hn => h n (.inr (.inl hn)),
      ihq fun n hn => h n (.inr (.inr hn))]

theorem Ty.agree_of_eq {t : Ty} {σ σ' : Nat → Ty} (h : t.subst σ = t.subst σ') :
    ∀ n, t.Occurs n → σ n = σ' n := by
  induction t with
  | var m => intro n hn; cases hn; exact h
  | con c => intro n hn; cases hn
  | cnil => intro n hn; cases hn
  | app t u iht ihu =>
    simp only [Ty.subst, Ty.app.injEq] at h
    intro n hn; rcases hn with hn | hn
    · exact iht h.1 n hn
    · exact ihu h.2 n hn
  | cun t u iht ihu =>
    simp only [Ty.subst, Ty.cun.injEq] at h
    intro n hn; rcases hn with hn | hn
    · exact iht h.1 n hn
    · exact ihu h.2 n hn
  | arr φ a b q iha ihb ihq =>
    simp only [Ty.subst, Ty.arr.injEq, true_and] at h
    intro n hn; rcases hn with hn | hn | hn
    · exact iha h.1 n hn
    · exact ihb h.2.1 n hn
    · exact ihq h.2.2 n hn

theorem params_subst (ts : List Ty) (θ : Nat → Ty) (n : Nat) :
    (params ts n).subst θ = params (ts.map (·.subst θ)) n := by
  induction ts generalizing n with
  | nil => rfl
  | cons t ts ih => cases n with
    | zero => rfl
    | succ n => exact ih n

theorem Prog.super_subst {P : Prog} {Q S : Cn} {i : Nat} (θ : Nat → Ty) (h : P.super Q i = some S) :
    P.super (Q.subst θ) i = some (S.subst θ) := by
  unfold Prog.super at h ⊢
  cases hs : (P.supers Q.cls)[i]? with
  | none => rw [hs] at h; cases h
  | some S₀ =>
    rw [hs] at h; simp only [Option.map_some, Option.some.injEq] at h; subst h
    simp only [Cn.subst, hs, Option.map_some, Option.some.injEq, Cn.mk.injEq, true_and,
      List.map_map]
    refine List.map_congr_left fun t _ => ?_
    simp only [Function.comp, Ty.subst_comp, params_subst]

theorem apps_subst (ρ : Nat → Dict) (f : Dict) (ds : List Dict) :
    (apps f ds).subst ρ = apps (f.subst ρ) (ds.map (·.subst ρ)) := by
  induction ds generalizing f with
  | nil => rfl
  | cons d ds ih => exact ih (.ap f d)

theorem Eqv.apps {P : Prog} {f g : Dict} {ds es : List Dict} (hf : Eqv P f g)
    (h : All2 (Eqv P) ds es) : Eqv P (apps f ds) (apps g es) := by
  induction h generalizing f g with
  | nil => exact hf
  | cons hd _ ih => exact ih (.ap hf hd)

variable {P : Prog}

theorem Clo.not_nil {d : Dict} {Q : Cn} (h : Clo P [] d Q) : False := by
  induction h with
  | given hk => simp at hk
  | super _ _ ih => exact ih

/-- A derivation of one constraint ends in one of the four rules. -/
theorem Ent.single {Δ : List Cn} {Q : Cn} {d : Dict} (h : Ent P Δ [Q] [d]) :
    Clo P Δ d Q ∨
    (∃ p, P.primC Q.cls = true ∧ P.prims Q = some p ∧ d = .prim p Q) ∨
    (∃ C τ ts ds, Q = ⟨C, [τ]⟩ ∧ P.structC C = true ∧ P.fields τ = some ts ∧
      Ent P Δ (ts.map fun t => ⟨C, [t]⟩) ds ∧ d = apps (.strct C τ) ds) ∨
    (∃ I ins σ ds, P.insts I = some ins ∧ ins.cls = Q.cls ∧ Q.args = ins.head.map (·.subst σ) ∧
      Ent P Δ (ins.ctx.map (·.subst σ)) ds ∧ d = apps (.inst I) ds) := by
  generalize hq : [Q] = qs at h
  generalize hd : [d] = ds at h
  induction h with
  | nil => cases hq
  | cons h₁ h₂ ih₁ _ =>
    cases hq; cases hd
    exact ih₁ rfl rfl
  | given hc => cases hq; cases hd; exact .inl hc
  | prim h₁ h₂ => cases hq; cases hd; exact .inr (.inl ⟨_, h₁, h₂, rfl⟩)
  | strct h₁ h₂ h₃ => cases hq; cases hd; exact .inr (.inr (.inl ⟨_, _, _, _, rfl, h₁, h₂, h₃, rfl⟩))
  | inst h₁ h₂ h₃ h₄ => cases hq; cases hd; exact .inr (.inr (.inr ⟨_, _, _, _, h₁, h₂, h₃, h₄, rfl⟩))

/-- A list derivation gives each constraint its dictionary. -/
theorem Ent.get {Δ qs : List Cn} {ds : List Dict} (h : Ent P Δ qs ds) :
    ds.length = qs.length ∧ ∀ k q, qs[k]? = some q → Ent P Δ [q] [args ds k] := by
  induction h with
  | nil => exact ⟨rfl, fun _ _ h => by simp at h⟩
  | cons h₁ _ _ ih₂ =>
    refine ⟨by simp [ih₂.1], fun k q hk => ?_⟩
    cases k with
    | zero => simp at hk; subst hk; exact h₁
    | succ k => exact ih₂.2 k q (by simpa using hk)
  | given h => exact ⟨rfl, fun k q hk => by cases k <;> simp at hk; subst hk; exact .given h⟩
  | prim h₁ h₂ => exact ⟨rfl, fun k q hk => by cases k <;> simp at hk; subst hk; exact .prim h₁ h₂⟩
  | strct h₁ h₂ h₃ =>
    exact ⟨rfl, fun k q hk => by cases k <;> simp at hk; subst hk; exact .strct h₁ h₂ h₃⟩
  | inst h₁ h₂ h₃ h₄ =>
    exact ⟨rfl, fun k q hk => by cases k <;> simp at hk; subst hk; exact .inst h₁ h₂ h₃ h₄⟩

end Flick.Elab
