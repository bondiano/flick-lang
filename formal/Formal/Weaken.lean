import Formal.Covers

/-!
# Structural lemmas: store weakening and renaming
-/

namespace Flick

variable {S : Sig}

theorem PatTy.binds {fl p t Δ} (h : PatTy S fl p t Δ) : Δ.length = p.binds := by
  induction h <;> simp_all [Pat.binds]

theorem HPat.binds {ε₀ p θ Δ} (h : HPat S ε₀ p θ Δ) : Δ.length = p.binds := by
  cases h with
  | wild => rfl
  | var => rfl
  | ctor hp => exact hp.binds

theorem getElem?_lt {α} {l : List α} {i : Nat} {a : α} (h : l[i]? = some a) : i < l.length := by
  obtain ⟨hi, _⟩ := List.getElem?_eq_some_iff.mp h
  exact hi

/-- Store weakening: `Ψ ⊆ Ψ'` keeps every judgment. -/
theorem Typ.weakenStore {Ψ Γ e J} (h : Typ S Ψ Γ e J) (X : List Ty) : Typ S (Ψ ++ X) Γ e J := by
  induction h generalizing X with
  | loc hl => exact .loc (by rw [List.getElem?_append_left (getElem?_lt hl)]; exact hl)
  | var hi hts hT => exact .var hi hts hT
  | lam _ hT ih => exact .lam (ih X) hT
  | app _ _ hψ hT ih₁ ih₂ => exact .app (ih₁ X) (ih₂ X) hψ hT
  | let_ _ _ ih₁ ih₂ => exact .let_ (ih₁ X) (ih₂ X)
  | letGen _ _ ih₁ ih₂ =>
    refine .letGen ?_ (ih₂ X)
    rw [List.map_append]; exact ih₁ _
  | letrec _ _ ih₁ ih₂ => exact .letrec (ih₁ X) (ih₂ X)
  | fix _ hT ih => exact .fix (ih X) hT
  | lit => exact .lit
  | const hc hT => exact .const hc hT
  | ctor hK hT => exact .ctor hK hT
  | rnil => exact .rnil
  | rcons hR hl _ _ ih₁ ih₂ => exact .rcons hR hl (ih₁ X) (ih₂ X)
  | proj _ hf hT ih => exact .proj (ih X) hf hT
  | update _ hf _ ih₁ ih₂ => exact .update (ih₁ X) hf (ih₂ X)
  | match_ _ _ hc ih₁ ih₂ => exact .match_ (ih₁ X) (ih₂ X) hc
  | anil => exact .anil
  | acons hp _ _ ih₁ ih₂ => exact .acons hp (ih₁ X) (ih₂ X)
  | aguard hp _ _ _ ih₁ ih₂ ih₃ => exact .aguard hp (ih₁ X) (ih₂ X) (ih₃ X)
  | todo => exact .todo
  | task _ hψ hT ih => exact .task (ih X) hψ hT
  | await _ hφ hT ih => exact .await (ih X) hφ hT
  | raise _ hφ ih => exact .raise (ih X) hφ
  | raiseAdt _ hφ ih => exact .raiseAdt (ih X) hφ
  | errVal hv _ hE ih => exact .errVal hv (ih X) hE
  | tryR _ _ hφ hs ih₁ ih₂ => exact .tryR (ih₁ X) (ih₂ X) hφ hs
  | tryP _ _ ih₁ ih₂ => exact .tryP (ih₁ X) (ih₂ X)
  | handle hv _ _ hc hφ ih₁ ih₂ => exact .handle hv (ih₁ X) (ih₂ X) hc hφ
  | hnil hH => exact .hnil hH
  | hcons hp _ _ hH ih₁ ih₂ => exact .hcons hp (ih₁ X) (ih₂ X) hH
  | hguard hp _ _ _ hH ih₁ ih₂ ih₃ => exact .hguard hp (ih₁ X) (ih₂ X) (ih₃ X) hH
  | provideL _ _ h₁ h₂ ih₁ ih₂ => exact .provideL (ih₁ X) (ih₂ X) h₁ h₂
  | provideE _ _ h₁ h₂ ih₁ ih₂ => exact .provideE (ih₁ X) (ih₂ X) h₁ h₂
  | with_ hv _ _ h₁ h₂ ih₁ ih₂ => exact .with_ hv (ih₁ X) (ih₂ X) h₁ h₂
  | enil hρ => exact .enil hρ
  | econs hv hθ _ _ hρ ih₁ ih₂ => exact .econs hv hθ (ih₁ X) (ih₂ X) hρ
  | bind _ hρ ih => exact .bind (ih X) hρ
  | join _ _ hρ ih₁ ih₂ => exact .join (ih₁ X) (ih₂ X) hρ

/-- A renaming `ξ : Γ → Γ'`. -/
def RenOk (Γ Γ' : List Scheme) (ξ : Nat → Nat) : Prop :=
  ∀ (i : Nat) t, Γ[i]? = some t → Γ'[ξ i]? = some t

theorem RenOk.up {Γ Γ' ξ} (h : RenOk Γ Γ' ξ) (k : Nat) :
    RenOk (Γ.map (Scheme.up k)) (Γ'.map (Scheme.up k)) ξ := by
  intro i t hi
  simp only [List.getElem?_map, Option.map_eq_some_iff] at hi ⊢
  obtain ⟨s, hs, rfl⟩ := hi
  exact ⟨s, h _ _ hs, rfl⟩

theorem RenOk.lift {Γ Γ' ξ} (h : RenOk Γ Γ' ξ) (Δ : List Scheme) {n} (hn : Δ.length = n) :
    RenOk (Δ ++ Γ) (Δ ++ Γ') (liftR n ξ) := by
  intro i t hi
  subst hn
  unfold liftR
  split
  · rw [List.getElem?_append_left (by omega)] at hi ⊢; exact hi
  · rw [List.getElem?_append_right (by omega)] at hi ⊢
    simpa using h _ _ hi

theorem Typ.rename {Ψ Γ e J} (h : Typ S Ψ Γ e J) :
    ∀ {Γ' ξ}, RenOk Γ Γ' ξ → Typ S Ψ Γ' (e.rename ξ) J := by
  induction h with
  | var hi hts hT => intro _ _ hξ; exact .var (hξ _ _ hi) hts hT
  | lam _ hT ih => intro _ _ hξ; exact .lam (ih (hξ.lift [_] rfl)) hT
  | app _ _ hψ hT ih₁ ih₂ => intro _ _ hξ; exact .app (ih₁ hξ) (ih₂ hξ) hψ hT
  | let_ _ _ ih₁ ih₂ => intro _ _ hξ; exact .let_ (ih₁ hξ) (ih₂ (hξ.lift [_] rfl))
  | letGen _ _ ih₁ ih₂ =>
    intro _ _ hξ; exact .letGen (ih₁ (hξ.up _)) (ih₂ (hξ.lift [_] rfl))
  | letrec _ _ ih₁ ih₂ =>
    intro _ _ hξ; exact .letrec (ih₁ (hξ.lift [_, _] rfl)) (ih₂ (hξ.lift [_] rfl))
  | fix _ hT ih => intro _ _ hξ; exact .fix (ih (hξ.lift [_, _] rfl)) hT
  | lit => intro _ _ _; exact .lit
  | loc hl => intro _ _ _; exact .loc hl
  | const hc hT => intro _ _ _; exact .const hc hT
  | ctor hK hT => intro _ _ _; exact .ctor hK hT
  | rnil => intro _ _ _; exact .rnil
  | rcons hR hl _ _ ih₁ ih₂ => intro _ _ hξ; exact .rcons hR hl (ih₁ hξ) (ih₂ hξ)
  | proj _ hf hT ih => intro _ _ hξ; exact .proj (ih hξ) hf hT
  | update _ hf _ ih₁ ih₂ => intro _ _ hξ; exact .update (ih₁ hξ) hf (ih₂ hξ)
  | match_ _ _ hc ih₁ ih₂ =>
    intro _ ξ hξ
    exact .match_ (ih₁ hξ) (ih₂ hξ)
      (by rw [Tm.unguarded_rename]; exact hc.map (Homo.rename ξ))
  | anil => intro _ _ _; exact .anil
  | acons hp _ _ ih₁ ih₂ => intro _ _ hξ; exact .acons hp (ih₁ (hξ.lift _ (by simpa using hp.binds))) (ih₂ hξ)
  | aguard hp _ _ _ ih₁ ih₂ ih₃ =>
    intro _ _ hξ
    exact .aguard hp (ih₁ (hξ.lift _ (by simpa using hp.binds))) (ih₂ (hξ.lift _ (by simpa using hp.binds))) (ih₃ hξ)
  | todo => intro _ _ _; exact .todo
  | task _ hψ hT ih => intro _ _ hξ; exact .task (ih hξ) hψ hT
  | await _ hφ hT ih => intro _ _ hξ; exact .await (ih hξ) hφ hT
  | raise _ hφ ih => intro _ _ hξ; exact .raise (ih hξ) hφ
  | raiseAdt _ hφ ih => intro _ _ hξ; exact .raiseAdt (ih hξ) hφ
  | errVal hv _ hE ih => intro _ _ hξ; exact .errVal hv.rename (ih hξ) hE
  | tryR _ _ hφ hs ih₁ ih₂ => intro _ _ hξ; exact .tryR (ih₁ hξ) (ih₂ hξ) hφ hs
  | tryP _ _ ih₁ ih₂ => intro _ _ hξ; exact .tryP (ih₁ hξ) (ih₂ hξ)
  | handle hv _ _ hc hφ ih₁ ih₂ =>
    intro _ ξ hξ
    exact .handle hv.rename (ih₁ hξ) (ih₂ hξ) (hc.map (Homo.rename ξ) hv) hφ
  | hnil hH => intro _ _ _; exact .hnil hH
  | hcons hp _ _ hH ih₁ ih₂ =>
    intro _ _ hξ; exact .hcons hp (ih₁ (hξ.lift _ (by simpa using hp.binds))) (ih₂ hξ) hH
  | hguard hp _ _ _ hH ih₁ ih₂ ih₃ =>
    intro _ _ hξ
    exact .hguard hp (ih₁ (hξ.lift _ (by simpa using hp.binds))) (ih₂ (hξ.lift _ (by simpa using hp.binds))) (ih₃ hξ) hH
  | provideL _ _ h₁ h₂ ih₁ ih₂ => intro _ _ hξ; exact .provideL (ih₁ hξ) (ih₂ hξ) h₁ h₂
  | provideE _ _ h₁ h₂ ih₁ ih₂ => intro _ _ hξ; exact .provideE (ih₁ hξ) (ih₂ hξ) h₁ h₂
  | with_ hv _ _ h₁ h₂ ih₁ ih₂ =>
    intro _ _ hξ; exact .with_ hv.rename (ih₁ hξ) (ih₂ hξ) h₁ h₂
  | enil hρ => intro _ _ _; exact .enil hρ
  | econs hv hθ _ _ hρ ih₁ ih₂ =>
    intro _ ξ hξ; exact .econs hv.rename ((Homo.rename ξ).isEnv hθ) (ih₁ hξ) (ih₂ hξ) hρ
  | bind _ hρ ih => intro _ _ hξ; exact .bind (ih hξ) hρ
  | join _ _ hρ ih₁ ih₂ => intro _ _ hξ; exact .join (ih₁ hξ) (ih₂ hξ) hρ

end Flick
