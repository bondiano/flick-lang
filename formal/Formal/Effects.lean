import Formal.Soundness

/-!
# T2: effect rows (core.md §4)
-/

namespace Flick

variable {S : Sig}

/-- One step from a pure term is pure and keeps the store and the regions. -/
theorem step_pure {root Ψ Θ Λ μ e t l Λ' μ' e'} (h : Typ S Ψ Θ [] e (.ex t .pure))
    (hs : Step root l Λ μ e (.cfg Λ' μ' e')) : l = .pure ∧ μ' = μ ∧ Λ' = Λ := by
  cases hs with
  | pure | glob => exact ⟨rfl, rfl, rfl⟩
  | rt hE hh =>
    obtain ⟨_, _, _, _, hr, rel, _⟩ := ctx_typing _ hE hh.not_value h
    obtain rfl := rel.pure rfl
    exact (hh.not_pure hr).elim
  | askWith hE =>
    obtain ⟨_, _, _, _, hr, rel, _⟩ := ctx_typing _ hE ask_not_value h
    obtain rfl := rel.pure rfl
    exact (ask_inv hr).1.elim
  | askRoot hE =>
    obtain ⟨_, _, _, _, hr, rel, _⟩ := ctx_typing _ hE ask_not_value h
    obtain rfl := rel.pure rfl
    exact (ask_inv hr).1.elim
  | raise hE hF =>
    obtain ⟨_, _, _, _, hr, rel, _⟩ := ctx_typing _ hE (not_value_plug raise_not_value _) h
    obtain rfl := rel.pure rfl
    obtain ⟨_, _, _, _, hr', rel', _⟩ := frame_typing raise_not_value _ hF hr
    obtain rfl := rel'.pure rfl
    exact (raise_not_pure hr').elim
  | addFin hE =>
    obtain ⟨_, _, _, _, hr, rel, _⟩ := ctx_typing _ hE (fin_not_value (.inl rfl)) h
    obtain rfl := rel.pure rfl
    exact (fin_not_pure hr (.inl rfl)).elim
  | spawn hE =>
    obtain ⟨_, _, _, _, hr, rel, _⟩ := ctx_typing _ hE (fin_not_value (.inr rfl)) h
    obtain rfl := rel.pure rfl
    exact (fin_not_pure hr (.inr rfl)).elim

/-- **T2(a), purity.** A computation from `·;Ψ ⊢ e : τ ! P` makes only pure
steps, keeps the store and the regions, and does not end in `raise v`. -/
theorem T2a {root Ψ Λ μ e t Λ' μ' e'} (hroot : RootTy S root) (hμ : StoreTy S Ψ μ)
    (hb : CfgBelow Λ μ e) (hΨ : ∀ u ∈ Ψ, u.YF Λ.length)
    (h : Typ S Ψ Facts.age [] e (.ex t .pure)) (hs : Steps root (fun _ => True) Λ μ e Λ' μ' e') :
    Steps root (· = .pure) Λ μ e Λ' μ' e' ∧ μ' = μ ∧ Λ' = Λ ∧ ∀ v, e' ≠ .raise v := by
  induction hs generalizing Ψ t with
  | refl => exact ⟨.refl, rfl, rfl, fun v he => by subst he; exact raise_not_pure h⟩
  | step h₁ _ _ ih =>
    obtain ⟨hl, rfl, rfl⟩ := step_pure h h₁
    obtain ⟨X, hμ', h', hX⟩ := preservation hroot h hμ hb.main hb.mem hΨ h₁
    obtain ⟨hb', _⟩ := h₁.below hroot hb
    obtain ⟨hs', rfl, rfl, hr⟩ := ih hμ' hb' (fun u hu => by
      rcases List.mem_append.mp hu with hu | hu
      · exact hΨ u hu
      · exact hX u hu) (by simpa [Stage.substT] using h')
    exact ⟨.step h₁ hl hs', rfl, rfl, hr⟩

/-- An `ask_S` answered by `Σ₀` from a term with services `r` has `S ∈ r`. -/
theorem step_root {root Ψ Θ Λ μ e t r ε N o} (h : Typ S Ψ Θ [] e (.ex t (.rt r ε)))
    (hs : Step root (.root N) Λ μ e o) : r (.nm N) := by
  cases hs with
  | askRoot hE hl =>
    obtain ⟨_, _, _, _, hr, rel, _⟩ := ctx_typing _ hE ask_not_value h
    rcases rel.svc _ (ask_inv hr).1 with h | h
    · exact h
    · rw [domE_nm, hl] at h; cases h

theorem Row.kill_sub (ε : Row) (n : Nat) : Row.Sub (ε.substT (kill n)) ε := by
  intro x hx
  cases x with
  | nm => rcases hx with hx | ⟨b, _, hb⟩
          · exact hx
          · cases kill_asRow hb
  | atm a =>
    rcases hx with ⟨b, hb, h⟩ | ⟨b, _, hb⟩
    · obtain ⟨rfl, _⟩ := kill_reach h; exact hb
    · cases kill_asRow hb
  | ob _ a | need _ a => obtain ⟨b, hb, h⟩ := hx; obtain ⟨rfl, _⟩ := kill_reach h; exact hb
  | rv a => obtain ⟨b, hb, h⟩ := hx; cases kill_asRow h; exact hb

theorem Steps.mono {root : Roots} {Q Q' : Lbl → Prop} {Λ μ e Λ' μ' e'}
    (h : Steps root Q Λ μ e Λ' μ' e') (hQ : ∀ l, Q l → Q' l) : Steps root Q' Λ μ e Λ' μ' e' := by
  induction h with
  | refl => exact .refl
  | step h₁ hl _ ih => exact .step h₁ (hQ _ hl) ih

theorem Row.kill_subE (ε : Row) (n : Nat) : Row.Sub (ε.substE (kill n)) ε := by
  intro x hx
  cases x with
  | atm a => obtain ⟨b, hb, h⟩ := hx; obtain ⟨rfl, _⟩ := kill_reach h; exact hb
  | _ => rw [Row.substE_of_ne (by simp)] at hx; exact Row.kill_sub ε n _ hx

/-- **T2(b), rows.** From `·;Ψ ⊢ e : τ ! R⟨r; ε⟩`, every `ask_S` that reaches `Σ₀`
has `S ∈ r`, and a computation that ends in `raise v` has `v : Errors ε`. -/
theorem T2b {root Ψ Λ μ e t r ε Λ' μ' e'} (hroot : RootTy S root) (hμ : StoreTy S Ψ μ)
    (hb : CfgBelow Λ μ e) (hΨ : ∀ u ∈ Ψ, u.YF Λ.length)
    (h : Typ S Ψ Facts.age [] e (.ex t (.rt r ε)))
    (hs : Steps root (fun _ => True) Λ μ e Λ' μ' e') :
    Steps root (fun l => ∀ N, l = .root N → r (.nm N)) Λ μ e Λ' μ' e' ∧
      ∀ v, e' = .raise v → Value v → ∃ Ψ', Typ S Ψ' Facts.age [] v (.ex (.errors ε) .pure) := by
  induction hs generalizing Ψ t r ε with
  | refl => exact ⟨.refl, fun v he hv => ⟨Ψ, by subst he; exact raise_errors hv h⟩⟩
  | step h₁ _ _ ih =>
    obtain ⟨X, hμ', h', hX⟩ := preservation hroot h hμ hb.main hb.mem hΨ h₁
    obtain ⟨hb', hl⟩ := h₁.below hroot hb
    obtain ⟨hs', hr⟩ := ih hμ' hb' (fun u hu => by
      rcases List.mem_append.mp hu with hu | hu
      · exact (hΨ u hu).mono hl
      · exact (hX u hu).mono hl) (by simpa only [Stage.substT] using h')
    refine ⟨.step h₁ (fun N hl => by subst hl; exact step_root h h₁)
      (hs'.mono fun _ hq N hl => Row.kill_sub _ _ _ (hq N hl)), fun v he hv => ?_⟩
    obtain ⟨Ψ', hv'⟩ := hr v he hv
    obtain ⟨_, _, _, _, U, _, _, _, hU, ht, hsub⟩ := canonical_errors hv hv'
    exact ⟨Ψ', .errVal hv ht hU (hsub.trans (Row.kill_subE _ _))⟩

/-- T2(b) for `await v` with `v : Eff r ε τ`. -/
theorem T2b_await {root Ψ Λ μ v r ε t C Λ' μ' e'} (hroot : RootTy S root) (hμ : StoreTy S Ψ μ)
    (hb : CfgBelow Λ μ (.await v)) (hΨ : ∀ u ∈ Ψ, u.YF Λ.length)
    (h : Typ S Ψ Facts.age [] v (.ex (.eff r ε t C) .pure))
    (hs : Steps root (fun _ => True) Λ μ (.await v) Λ' μ' e') :
    Steps root (fun l => ∀ N, l = .root N → r (.nm N)) Λ μ (.await v) Λ' μ' e' ∧
      ∀ w, e' = .raise w → Value w → ∃ Ψ', Typ S Ψ' Facts.age [] w (.ex (.errors ε) .pure) :=
  T2b hroot hμ hb hΨ (.await (h.sub ⟨.refl, Stage.pure_le _⟩) (Stage.le_refl _) .refl) hs

end Flick
