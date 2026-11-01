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

theorem raise_not_pure {Ψ e t} (h : Typ S Ψ [] (.raise e) (.ex t .pure)) : False := by
  obtain ⟨_, hε⟩ := raise_stage h; exact hε

/-- No runtime redex has a pure stage. -/
theorem RHead.not_pure {Ψ μ r t μ' r'} (h : Typ S Ψ [] r (.ex t .pure)) (hh : RHead μ r μ' r') :
    False := by
  have nv := hh.not_value
  cases hh with
  | refMake | refGet =>
    cases h with
    | app hf _ hψ _ =>
      cases hf with
      | const _ hT' =>
        obtain ⟨_, _, _, h, _, lψ, _⟩ := hT'.arrow_left
        cases h; exact Stage.le_trans lψ hψ
    | errVal hv => exact nv hv
  | refSet =>
    cases h with
    | app hf _ hψ _ =>
      cases hf with
      | app hc _ _ hT =>
        cases hc with
        | const _ hT' =>
          obtain ⟨_, _, _, h, _, _, lt⟩ := hT'.arrow_left
          cases h
          obtain ⟨_, _, _, h, _, lψ, _⟩ := (lt.trans hT).arrow_left
          cases h; exact Stage.le_trans lψ hψ
    | errVal hv => exact nv hv
  | await => cases h with
    | await _ hφ => exact hφ
    | errVal hv => exact nv hv
  | build => cases h with
    | provideL _ _ _ hφ => exact hφ
    | provideE _ _ _ hφ => exact hφ
    | errVal hv => exact nv hv
  | provideEnv => cases h with
    | provideL _ _ _ hφ => exact hφ
    | provideE _ _ _ hφ => exact hφ
    | errVal hv => exact nv hv
  | tryRaise => cases h with
    | tryR _ _ hφ => exact hφ
    | tryP hb => exact raise_not_pure hb
    | errVal hv => exact nv hv
  | handleYes | handleNo | hguardYes | hguardNo | handleNil => cases h with
    | handle _ _ _ _ hφ => exact hφ
    | errVal hv => exact nv hv

/-- A hole that is not pure is not under a (Let-Gen) right-hand side. -/
theorem depth_zero {K : Nat} {φ' : Stage} (hK : K ≠ 0 → φ' = .pure) (hp : φ' ≠ .pure) : K = 0 := by
  by_cases h : K = 0
  · exact h
  · exact (hp (hK h)).elim

/-- Removing a frame around `raise v` keeps the type of the hole. -/
theorem raise_frame {Ψ v t φ} {F : Frame} (hF : F.ok) (hnt : ¬ F.isTry) (hv : Value v)
    (h : Typ S Ψ [] (F.plug (.raise v)) (.ex t φ)) : Typ S Ψ [] (.raise v) (.ex t φ) := by
  obtain ⟨k, _, φ', hr, rel, hk, _, _⟩ := frame_typing raise_not_value F hF h
  obtain ⟨ε, hv', hε⟩ := raise_inv hv hr
  have hne : φ' ≠ .pure := by intro h; subst h; exact hε
  obtain rfl := depth_zero hk hne
  simp only [List.map_up_zero] at hv' 
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
    obtain ⟨_, _, _, hr, _, _, back, _⟩ := ctx_typing _ hE hh.not_value h
    exact ⟨[], by simpa using hμ, back [] _ (by simpa using pure_preservation hr hh)⟩
  | rt hE hh =>
    obtain ⟨K, _, _, hr, _, hK, back, _⟩ := ctx_typing _ hE hh.not_value h
    obtain rfl := depth_zero hK fun hp => by subst hp; exact hh.not_pure hr
    simp only [List.map_up_zero] at hr
    obtain ⟨X, hμ', hr'⟩ := rt_preservation hr hμ hh
    exact ⟨X, hμ', back X _ (by simpa using hr')⟩
  | askWith hE hl =>
    obtain ⟨K, t₁, φ₁, hr, _, hK, back, look⟩ := ctx_typing _ hE ask_not_value h
    obtain rfl := depth_zero hK fun hp => by subst hp; exact (ask_inv hr).1
    simp only [List.map_up_zero] at hr
    have hv := (look rfl _ _ hl).sub (J' := .ex t₁ φ₁) ⟨(ask_inv hr).2, Stage.pure_le _⟩
    exact ⟨[], by simpa using hμ, back [] _ (by simpa using hv)⟩
  | askRoot hE _ hr₀ =>
    obtain ⟨K, t₁, φ₁, hr, _, hK, back, _⟩ := ctx_typing _ hE ask_not_value h
    obtain rfl := depth_zero hK fun hp => by subst hp; exact (ask_inv hr).1
    simp only [List.map_up_zero] at hr
    have hv := ((hroot _ _ hr₀).2.weakenStore Ψ).sub (J' := .ex t₁ φ₁)
      ⟨(ask_inv hr).2, Stage.pure_le _⟩
    exact ⟨[], by simpa using hμ, back [] _ (by simpa using hv)⟩
  | raise hE hF hnt hv =>
    obtain ⟨K, _, _, hr, _, hK, back, _⟩ :=
      ctx_typing _ hE (not_value_plug raise_not_value _) h
    obtain rfl := depth_zero hK fun hp => by
      subst hp
      obtain ⟨_, _, _, hr', rel', _⟩ := frame_typing raise_not_value _ hF hr
      obtain rfl := rel'.pure rfl
      exact raise_not_pure hr'
    simp only [List.map_up_zero] at hr
    exact ⟨[], by simpa using hμ, back [] _ (by simpa using raise_frame hF hnt hv hr)⟩

end Flick
