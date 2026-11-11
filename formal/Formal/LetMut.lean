import Formal.LetMut.Core

namespace Flick

/-! ## T7 -/

theorem Exh.wild {t : Ty} {p : Pat} : Exh S t [p, .wild] :=
  fun _ _ => ⟨.wild, by simp, by simp [matchP]⟩

theorem finTy_cons (s : Scheme) (Γ : List Scheme) (fin : Option (List Nat)) (t : Ty) :
    finTy (s :: Γ) (fin.map (·.map (· + 1))) t = finTy Γ fin t := by
  cases fin <;> simp [finTy, stateTy_cons]

/-- The typing of the translation, for any state a block's value comes with. -/
theorem MutOk.typ {Ψ Θ Γ ms b t φ} (h : MutOk S Ψ Θ Γ ms b t φ) (hb : ¬ b.Has .ins) :
    VarsOk Γ ms → ∀ fin : Option (List Nat), (∀ fs, fin = some fs → VarsOk Γ fs) →
      Typ S Ψ Θ Γ (tr fin ms b) (.ex (finTy Γ fin t) φ) := by
  have shifted : ∀ {Γ : List Scheme} (s : Scheme) (fin : Option (List Nat)),
      (∀ fs, fin = some fs → VarsOk Γ fs) →
        ∀ fs, fin.map (·.map (· + 1)) = some fs → VarsOk (s :: Γ) fs := by
    intro Γ s fin hfin fs h
    cases fin with
    | none => cases h
    | some fs' => cases h; exact (hfin _ rfl).cons s
  induction h with
  | ret he =>
    intro _ fin hfin
    cases fin with
    | none => exact he
    | some fs => exact .pack he (Typ.state (hfin fs rfl))
  | let_ he _ ih =>
    intro hv fin hfin
    have := ih (fun h => hb (.inr h)) (hv.cons _) _ (shifted _ fin hfin)
    rw [finTy_cons] at this
    exact .let_ he this
  | @var Ψ Θ Γ ms e b a t φ he _ ih =>
    intro hv fin hfin
    have := ih (fun h => hb (.inr h)) (hv.var a) _ (shifted _ fin hfin)
    rw [finTy_cons] at this
    exact .let_ he this
  | set _ hx he _ ih =>
    intro hv fin hfin
    exact .let_ he ((ih (fun h => hb (.inr h)) hv fin hfin).rename (RenOk.redirect hx))
  | @scope Ψ Θ Γ ms a b t t₁ φ φ' r ε F hF _ hφ' hφ _ iha ihb =>
    intro hv fin hfin
    have ha := iha (fun h => hb (.inl h)) (hv.scope _) (some (ms.map (· + 1))) (by
      intro fs h; cases h; exact hv.scope _)
    simp only [finTy, stateTy_up hv, ← packTy_up] at ha
    have hn : (tr (some (ms.map (· + 1))) (ms.map (· + 1)) a).NoIns :=
      fun h => hb (.inl (tr_has h))
    have hk := ihb (fun h => hb (.inr h)) (hv.cons _) _ (shifted _ fin hfin)
    rw [finTy_cons] at hk
    exact .let_ (.scope hn hF ha hφ' hφ .refl) (.unpack hv hk)
  | @br Ψ Θ Γ ms e p a a' b u t t₁ φ Δ he hp _ _ _ iha iha' ihb =>
    intro hv fin hfin
    have hl := hp.binds
    have hvΔ : VarsOk (Δ.map Scheme.mono ++ Γ) (ms.map (· + Δ.length)) := by
      simpa using hv.append (Δ.map Scheme.mono)
    have ha := iha (fun h => hb (.inr (.inl h))) hvΔ (some (ms.map (· + Δ.length))) (by
      intro fs h; cases h; exact hvΔ)
    have ha' := iha' (fun h => hb (.inr (.inr (.inl h)))) hv (some ms) (by
      intro fs h; cases h; exact hv)
    have hst : stateTy (Δ.map Scheme.mono ++ Γ) (ms.map (· + Δ.length)) = stateTy Γ ms := by
      simpa using stateTy_append (Δ.map Scheme.mono) Γ ms
    simp only [finTy, hst] at ha ha'
    rw [hl] at ha
    have hk := ihb (fun h => hb (.inr (.inr (.inr h)))) (hv.cons _) _ (shifted _ fin hfin)
    rw [finTy_cons] at hk
    exact .let_ (.match_ he (.acons hp ha (.acons PatTy.wild (by simpa using ha') .anil))
      (.inl Exh.wild)) (.unpack hv hk)

/-- **T7**: a block of a function with `let mut` variables translates to a term of the
core with the block's type at the block's stage. -/
theorem T7 {Ψ Θ Γ b t φ} (h : MutOk S Ψ Θ Γ [] b t φ) (hb : ¬ b.Has .ins) :
    Typ S Ψ Θ Γ (tr none [] b) (.ex t φ) :=
  h.typ hb (fun _ h => nomatch h) none (fun _ h => nomatch h)

theorem tr_below {n : Nat} {b : Blk} (hb : ∀ p, b.Has (.rgn p) → p < n) :
    (tr none [] b).Below n :=
  fun p h => hb p (tr_has h)

/-- T2a for a pure block: its translation makes only pure steps, which change no cell
and no region, and raises nothing. -/
theorem T7_pure {root Ψ Λ μ b t Λ' μ' e'} (hroot : RootTy S root) (hμ : StoreTy S Ψ μ)
    (hb : ∀ p, b.Has (.rgn p) → p < Λ.length) (hins : ¬ b.Has .ins)
    (hmem : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Below Λ.length)
    (hrgs : ∀ (p : Nat) (g : Rgn), Λ[p]? = some g →
      (∀ k ∈ g.kids, k.Below Λ.length) ∧ ∀ f ∈ g.fins, f.Below Λ.length)
    (hΨ : ∀ u ∈ Ψ, u.YF Λ.length) (h : MutOk S Ψ Facts.age [] [] b t .pure)
    (hs : Steps root (fun _ => True) Λ μ (tr none [] b) Λ' μ' e') :
    Steps root (· = .pure) Λ μ (tr none [] b) Λ' μ' e' ∧ μ' = μ ∧ Λ' = Λ ∧ ∀ v, e' ≠ .raise v :=
  T2a hroot hμ ⟨tr_below hb, hmem, hrgs⟩ hΨ (T7 h hins) hs

/-- T4′ for the translated closed block: every region use and installed handler
is live at its use. Holding an unused capability is allowed. -/
theorem T7_regions {root b t φ₀ c} (hroot : RootTy S root) (h : MutOk S [] Facts.none [] [] b t φ₀)
    (hins : ¬ b.Has .ins) (hp : ∀ p, ¬ b.Has (.rgn p)) (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hs : CSteps root ⟨[], [], tr none [] b⟩ c) :
    (∀ E r, OkE E → c.main = plug E r → ¬ DeadUse c.rgs r) ∧
    (∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → g.live = true → ∀ k ∈ g.kids, ∀ E r, OkE E → k = plug E r →
      ¬ DeadUse c.rgs r) ∧
    ∀ i e E Ef o k v q, c.at i = some e → OkE E → e = plug E (.app (.const (.perform Ef o k)) v) →
      handlerOf E Ef o k = some q → Live c.rgs q :=
  T4' hroot (tr none [] b) t φ₀ c (T7 h hins)
    (tr_below fun p h => absurd h (hp p)) ht hφ hs

end Flick
