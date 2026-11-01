import Formal.Soundness

/-!
# T2: effect rows (core.md §4)
-/

namespace Flick

variable {S : Sig}

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

/-- One step from a pure term is pure and keeps the store. -/
theorem step_pure {root Ψ μ e t l μ' e'} (h : Typ S Ψ [] e (.ex t .pure))
    (hs : Step root l μ e (.cfg μ' e')) : l = .pure ∧ μ' = μ := by
  cases hs with
  | pure => exact ⟨rfl, rfl⟩
  | rt hE hh =>
    obtain ⟨_, _, hr, rel, _⟩ := ctx_typing _ hE hh.not_value h
    obtain rfl := rel.pure rfl
    exact (hh.not_pure hr).elim
  | askWith hE =>
    obtain ⟨_, _, hr, rel, _⟩ := ctx_typing _ hE ask_not_value h
    obtain rfl := rel.pure rfl
    exact (ask_inv hr).1.elim
  | askRoot hE =>
    obtain ⟨_, _, hr, rel, _⟩ := ctx_typing _ hE ask_not_value h
    obtain rfl := rel.pure rfl
    exact (ask_inv hr).1.elim
  | raise hE hF =>
    obtain ⟨_, _, hr, rel, _⟩ := ctx_typing _ hE (not_value_plug raise_not_value _) h
    obtain rfl := rel.pure rfl
    obtain ⟨_, _, hr', rel', _⟩ := frame_typing raise_not_value _ hF hr
    obtain rfl := rel'.pure rfl
    exact (raise_not_pure hr').elim

/-- **T2(a), purity.** A computation from `·;Ψ ⊢ e : τ ! P` makes only pure
steps, keeps the store and does not end in `raise v`. -/
theorem T2a {root Ψ μ e t μ' e'} (hroot : RootTy S root) (hμ : StoreTy S Ψ μ)
    (h : Typ S Ψ [] e (.ex t .pure)) (hs : Steps root (fun _ => True) μ e μ' e') :
    Steps root (· = .pure) μ e μ' e' ∧ μ' = μ ∧ ∀ v, e' ≠ .raise v := by
  induction hs generalizing Ψ with
  | refl => exact ⟨.refl, rfl, fun v he => by subst he; exact raise_not_pure h⟩
  | step h₁ _ _ ih =>
    obtain ⟨hl, rfl⟩ := step_pure h h₁
    obtain ⟨X, hμ', h'⟩ := preservation hroot h hμ h₁
    obtain ⟨hs', rfl, hr⟩ := ih hμ' h'
    exact ⟨.step h₁ hl hs', rfl, hr⟩

/-- An `ask_S` answered by `Σ₀` from a term with services `r` has `S ∈ r`. -/
theorem step_root {root Ψ μ e t r ε N o} (h : Typ S Ψ [] e (.ex t (.rt r ε)))
    (hs : Step root (.root N) μ e o) : r N := by
  cases hs with
  | askRoot hE hl =>
    obtain ⟨_, _, hr, rel, _⟩ := ctx_typing _ hE ask_not_value h
    rcases rel.svc N (ask_inv hr).1 with h | h
    · exact h
    · simp [domE, hl] at h

/-- **T2(b), rows.** From `·;Ψ ⊢ e : τ ! R⟨r; ε⟩`, every `ask_S` that reaches `Σ₀`
has `S ∈ r`, and a computation that ends in `raise v` has `v : Errors ε`. -/
theorem T2b {root Ψ μ e t r ε μ' e'} (hroot : RootTy S root) (hμ : StoreTy S Ψ μ)
    (h : Typ S Ψ [] e (.ex t (.rt r ε))) (hs : Steps root (fun _ => True) μ e μ' e') :
    Steps root (fun l => ∀ N, l = .root N → r N) μ e μ' e' ∧
      ∀ v, e' = .raise v → Value v → ∃ Ψ', Typ S Ψ' [] v (.ex (.errors ε) .pure) := by
  induction hs generalizing Ψ with
  | refl => exact ⟨.refl, fun v he hv => ⟨Ψ, by subst he; exact raise_errors hv h⟩⟩
  | step h₁ _ _ ih =>
    obtain ⟨X, hμ', h'⟩ := preservation hroot h hμ h₁
    obtain ⟨hs', hr⟩ := ih hμ' h'
    exact ⟨.step h₁ (fun N hl => by subst hl; exact step_root h h₁) hs', hr⟩

/-- T2(b) for `await v` with `v : Eff r ε τ`. -/
theorem T2b_await {root Ψ μ v r ε t μ' e'} (hroot : RootTy S root) (hμ : StoreTy S Ψ μ)
    (h : Typ S Ψ [] v (.ex (.eff r ε t) .pure))
    (hs : Steps root (fun _ => True) μ (.await v) μ' e') :
    Steps root (fun l => ∀ N, l = .root N → r N) μ (.await v) μ' e' ∧
      ∀ w, e' = .raise w → Value w → ∃ Ψ', Typ S Ψ' [] w (.ex (.errors ε) .pure) :=
  T2b hroot hμ (.await (h.sub ⟨.refl, Stage.pure_le _⟩) (Stage.le_refl _) .refl) hs

end Flick
