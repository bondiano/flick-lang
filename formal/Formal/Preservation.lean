import Formal.PresRt

/-!
# Preservation (core.md §4)

A step keeps the type and the stage, and grows the store typing. The stage is
kept exactly: (Sub) lifts a smaller one.
-/

namespace Flick

variable {S : Sig}

/-- `raise v` takes a runtime stage whose errors hold those of `v`. -/
theorem raise_stage {Ψ e t φ} (h : Typ S Ψ [] (.raise e) (.ex t φ)) :
    ∃ ε, (Stage.rt Row.none ε).le φ := by
  cases h with
  | raise _ hφ => exact ⟨_, hφ⟩
  | raiseAdt _ hφ => exact ⟨_, hφ⟩
  | errVal hv => cases hv

/-- `ask_S ()` takes a stage with `S`, and its type is above `S.Impl`. -/
theorem ask_inv {Ψ N w t φ} (h : Typ S Ψ [] (.app (.const (.ask N)) w) (.ex t φ)) :
    φ.svc N ∧ Le (S.impl N) t := by
  cases h with
  | app hf _ hψ hT =>
    cases hf with
    | const _ hT' =>
      obtain ⟨_, _, _, h, _, lψ, lt⟩ := hT'.arrow_left
      cases h
      exact ⟨Stage.le_svc (Stage.le_trans lψ hψ) N rfl, lt.trans hT⟩
  | errVal hv => exact (ask_not_value hv).elim

/-- Removing a frame around `raise v` keeps the type of the hole. -/
theorem raise_frame {Ψ v t φ} {F : Frame} (hF : F.ok) (hnt : ¬ F.isTry) (hv : Value v)
    (h : Typ S Ψ [] (F.plug (.raise v)) (.ex t φ)) : Typ S Ψ [] (.raise v) (.ex t φ) := by
  obtain ⟨_, φ', hr, rel, _, _⟩ := frame_typing raise_not_value F hF h
  obtain ⟨ε, hv', hε⟩ := raise_inv hv hr
  have hne : φ' ≠ .pure := by intro h; subst h; exact hε
  obtain ⟨_, _, rfl, _, he⟩ := Stage.rt_le hε
  cases hφ : φ with
  | pure => exact (rel.rt hnt hne hφ).elim
  | rt r e =>
    subst hφ
    exact .raise (hv'.valueAny hv _) ⟨fun _ h => h.elim, fun n h => rel.err hnt n (he n h)⟩

theorem preservation {root Ψ μ e t φ l μ' e'} (hroot : RootTy S root)
    (h : Typ S Ψ [] e (.ex t φ)) (hμ : StoreTy S Ψ μ) (hs : Step root l μ e (.cfg μ' e')) :
    ∃ X, StoreTy S (Ψ ++ X) μ' ∧ Typ S (Ψ ++ X) [] e' (.ex t φ) := by
  cases hs with
  | pure hE hh =>
    obtain ⟨_, _, hr, _, back, _⟩ := ctx_typing _ hE hh.not_value h
    exact ⟨[], by simpa using hμ, back [] _ (by simpa using pure_preservation hr hh)⟩
  | rt hE hh =>
    obtain ⟨_, _, hr, _, back, _⟩ := ctx_typing _ hE hh.not_value h
    obtain ⟨X, hμ', hr'⟩ := rt_preservation hr hμ hh
    exact ⟨X, hμ', back X _ hr'⟩
  | askWith hE hl =>
    obtain ⟨t₁, φ₁, hr, _, back, look⟩ := ctx_typing _ hE ask_not_value h
    have hv := (look _ _ hl).sub (J' := .ex t₁ φ₁) ⟨(ask_inv hr).2, Stage.pure_le _⟩
    exact ⟨[], by simpa using hμ, back [] _ (by simpa using hv)⟩
  | askRoot hE _ hr₀ =>
    obtain ⟨t₁, φ₁, hr, _, back, _⟩ := ctx_typing _ hE ask_not_value h
    have hv := ((hroot _ _ hr₀).2.weakenStore Ψ).sub (J' := .ex t₁ φ₁)
      ⟨(ask_inv hr).2, Stage.pure_le _⟩
    exact ⟨[], by simpa using hμ, back [] _ (by simpa using hv)⟩
  | raise hE hF hnt hv =>
    obtain ⟨_, _, hr, _, back, _⟩ := ctx_typing _ hE (not_value_plug raise_not_value _) h
    exact ⟨[], by simpa using hμ, back [] _ (by simpa using raise_frame hF hnt hv hr)⟩

end Flick
