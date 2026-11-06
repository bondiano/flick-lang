import Formal.Elab.CoreTr
import Formal.SubstComp
import Formal.Closed

/-!
# λFlick⁺ as a `Target`: closing the premises of a translation

Closing the premises of an elaborated term from `L` on by closed dictionaries (`Tm.dsubst`)
is a substitution of its translation (`Tr.dsubst`): the variables go to variables, the
premises below `L` to premises, the others to the translations of their dictionaries, which
name no premise and so are the same under every binder (`trDt_below`).
-/

namespace Flick.Elab

variable {N : Names}

theorem liftS_succ (m : Nat) (σ : Nat → Flick.Tm) (i : Nat) :
    liftS m (liftS 1 σ) i = liftS (m + 1) σ i := by
  simp only [liftS]
  by_cases h : i < m
  · rw [if_pos h, if_pos (show i < m + 1 by omega)]
  · rw [if_neg h]
    by_cases h' : i < m + 1
    · rw [if_pos h', if_pos (by omega : i - m < 1)]; simp only [Flick.Tm.rename]; congr 1; omega
    · rw [if_neg h', if_neg (by omega : ¬ i - m < 1), Flick.Tm.rename_rename,
        show i - m - 1 = i - (m + 1) by omega]
      congr 1; funext j; simp only [Function.comp]; omega

theorem lamsC_subst (m : Nat) (b : Flick.Tm) (σ : Nat → Flick.Tm) :
    (lamsC m b).subst σ = lamsC m (b.subst (liftS m σ)) := by
  induction m generalizing σ with
  | zero =>
    simp only [lamsC]; congr 1; funext i; simp only [liftS, Nat.not_lt_zero, if_false, Nat.sub_zero]
    exact ((σ i).rename_fix _ fun _ _ => rfl).symm
  | succ m ih =>
    simp only [lamsC, Flick.Tm.subst]
    rw [ih, show liftS m (liftS 1 σ) = liftS (m + 1) σ from funext (liftS_succ m σ)]

theorem trDt_rename (ξ δ : Nat → Nat) (d : Dict) :
    (trDt N δ d).rename ξ = trDt N (fun k => ξ (δ k)) d := by
  induction d with
  | var k => rfl
  | slot d i ih => simp [trDt, Flick.Tm.rename, ih]
  | ap d e ihd ihe => simp [trDt, Flick.Tm.rename, ihd, ihe]
  | _ => rfl

theorem trDt_below {δ δ' : Nat → Nat} : ∀ {d : Dict}, d.Below 0 → trDt N δ d = trDt N δ' d
  | .var k, h => absurd h (Nat.not_lt_zero k)
  | .slot d _, h => by simp only [trDt]; rw [trDt_below (δ := δ) (δ' := δ') (d := d) h]
  | .ap d e, h => by
    simp only [trDt]
    rw [trDt_below (δ := δ) (δ' := δ') (d := d) h.1, trDt_below (δ := δ) (δ' := δ') (d := e) h.2]
  | .inst _, _ | .strct _ _, _ | .prim _ _, _ => rfl

/-- The variables under `l` more binders, `j` of them from the term. -/
theorem ghyp_lift {τ : Nat → Flick.Tm} {γ₁ γ₂ γ₁' γ₂' : Nat → Nat} {j l : Nat}
    (h : ∀ x, τ (γ₁ x) = .var (γ₂ x)) (hin : ∀ x, x < j → γ₁' x < l ∧ γ₂' x = γ₁' x)
    (hout : ∀ x, j ≤ x → γ₁' x = γ₁ (x - j) + l ∧ γ₂' x = γ₂ (x - j) + l) :
    ∀ x, liftS l τ (γ₁' x) = .var (γ₂' x) := by
  intro x
  by_cases hx : x < j
  · obtain ⟨h₁, h₂⟩ := hin x hx
    simp [liftS, h₁, h₂]
  · obtain ⟨h₁, h₂⟩ := hout x (by omega)
    simp only [liftS, h₁, h₂, if_neg (by omega : ¬ γ₁ (x - j) + l < l), Nat.add_sub_cancel, h]
    rfl

/-- The premises under `l` more binders, `j` of them premises of the term. -/
theorem dhyp_lift {ρ : Nat → Dict} (hρ : ∀ k, (ρ k).Below 0) {τ : Nat → Flick.Tm}
    {δ₁ δ₂ δ₁' δ₂' : Nat → Nat} {L j l : Nat} (h : ∀ k, τ (δ₁ k) = trDt N δ₂ (dsub L ρ k))
    (hin : ∀ k, k < j → δ₁' k < l ∧ δ₂' k = δ₁' k)
    (hout : ∀ k, j ≤ k → δ₁' k = δ₁ (k - j) + l ∧ δ₂' k = δ₂ (k - j) + l) :
    ∀ k, liftS l τ (δ₁' k) = trDt N δ₂' (dsub (j + L) ρ k) := by
  intro k
  by_cases hk : k < j
  · obtain ⟨h₁, h₂⟩ := hin k hk
    simp [liftS, h₁, h₂, dsub, trDt, show k < j + L by omega]
  · obtain ⟨h₁, h₂⟩ := hout k (by omega)
    simp only [liftS, h₁, if_neg (by omega : ¬ δ₁ (k - j) + l < l), Nat.add_sub_cancel, h,
      trDt_rename]
    by_cases hL : k - j < L
    · simp [dsub, hL, trDt, h₂, show k < j + L by omega]
    · simp only [dsub, if_neg hL, if_neg (show ¬ k < j + L by omega),
        show k - j - L = k - (j + L) by omega]
      exact trDt_below (hρ _)

theorem hyps_zero {ρ : Nat → Dict} (hρ : ∀ k, (ρ k).Below 0) {τ : Nat → Flick.Tm}
    {γ₁ γ₂ δ₁ δ₂ : Nat → Nat} {L : Nat} (hγ : ∀ x, τ (γ₁ x) = .var (γ₂ x))
    (hδ : ∀ k, τ (δ₁ k) = trDt N δ₂ (dsub L ρ k)) :
    (∀ x, liftS 0 τ (γ₁ x) = .var (γ₂ x)) ∧ (∀ k, liftS 0 τ (δ₁ k) = trDt N δ₂ (dsub L ρ k)) :=
  ⟨ghyp_lift (γ₁' := γ₁) (γ₂' := γ₂) (j := 0) hγ (fun _ h => absurd h (Nat.not_lt_zero _))
      fun _ _ => by simp,
    fun k => by
      simpa using dhyp_lift (δ₁' := δ₁) (δ₂' := δ₂) (j := 0) (l := 0) hρ hδ
        (fun _ h => absurd h (Nat.not_lt_zero _)) (fun _ _ => by simp) k⟩

theorem hyps_one {ρ : Nat → Dict} (hρ : ∀ k, (ρ k).Below 0) {τ : Nat → Flick.Tm}
    {γ₁ γ₂ δ₁ δ₂ : Nat → Nat} {L : Nat} (hγ : ∀ x, τ (γ₁ x) = .var (γ₂ x))
    (hδ : ∀ k, τ (δ₁ k) = trDt N δ₂ (dsub L ρ k)) :
    (∀ x, liftS 1 τ (ext γ₁ x) = .var (ext γ₂ x)) ∧
      (∀ k, liftS 1 τ (δ₁ k + 1) = trDt N (fun k => δ₂ k + 1) (dsub L ρ k)) :=
  ⟨ghyp_lift (γ₁' := ext γ₁) (γ₂' := ext γ₂) (j := 1) hγ
      (fun x hx => by cases x <;> simp_all [ext]) fun x hx => by cases x <;> simp_all [ext],
    fun k => by
      simpa using dhyp_lift (δ₁' := fun k => δ₁ k + 1) (δ₂' := fun k => δ₂ k + 1) (j := 0) (l := 1)
        hρ hδ (fun _ h => absurd h (Nat.not_lt_zero _)) (fun _ _ => by simp) k⟩

/-- **Closing the premises is a substitution of the translation.** -/
theorem Tr.dsubst {ρ : Nat → Dict} (hρ : ∀ k, (ρ k).Below 0) {γ₁ δ₁ : Nat → Nat} {e : Tm}
    {e₁ : Flick.Tm} (h : Tr N γ₁ δ₁ e e₁) :
    ∀ {γ₂ δ₂ : Nat → Nat} {τ : Nat → Flick.Tm} {L : Nat}, (∀ x, τ (γ₁ x) = .var (γ₂ x)) →
      (∀ k, τ (δ₁ k) = trDt N δ₂ (dsub L ρ k)) → Tr N γ₂ δ₂ (e.dsubst ρ L) (e₁.subst τ) := by
  induction h with
  | var => intro γ₂ δ₂ τ L hγ _; simp only [Tm.dsubst, Flick.Tm.subst, hγ]; exact .var
  | dict hd => intro γ₂ δ₂ τ L _ hδ; rw [hd.eq, trDt_subst hδ]; exact .dict (TrD.total _ _)
  | meth _ ih => intro _ _ _ _ hγ hδ; exact .meth (ih hγ hδ)
  | const => intros; exact .const
  | con => intros; exact .con
  | app _ _ ih₁ ih₂ => intro _ _ _ _ hγ hδ; exact .app (ih₁ hγ hδ) (ih₂ hγ hδ)
  | lam _ ih =>
    intro _ _ _ _ hγ hδ
    obtain ⟨h₁, h₂⟩ := hyps_one hρ hγ hδ
    exact .lam (ih h₁ h₂)
  | let_ _ _ ih₁ ih₂ =>
    intro _ _ _ _ hγ hδ
    obtain ⟨h₁, h₂⟩ := hyps_one hρ hγ hδ
    exact .let_ (ih₁ hγ hδ) (ih₂ h₁ h₂)
  | @lamD γ δ l b b' _ ih =>
    intro γ₂ δ₂ τ L hγ hδ
    rw [lamsC_subst]
    refine .lamD (ih (ghyp_lift (j := 0) hγ (fun _ h => absurd h (Nat.not_lt_zero _))
      (fun _ _ => by simp)) (dhyp_lift (j := l) hρ hδ (fun k hk => ?_) fun k hk => ?_))
    · simp [dext, hk]; omega
    · simp [dext, show ¬ k < l by omega]
  | ite _ _ _ ihc iha ihb =>
    intro _ _ _ _ hγ hδ
    obtain ⟨h₁, h₂⟩ := hyps_zero hρ hγ hδ
    exact .ite (ihc hγ hδ) (iha h₁ h₂) (ihb h₁ h₂)
  | caseOpt _ _ _ ihe ihn ihs =>
    intro _ _ _ _ hγ hδ
    obtain ⟨h₁, h₂⟩ := hyps_one hρ hγ hδ
    obtain ⟨h₃, h₄⟩ := hyps_zero hρ hγ hδ
    exact .caseOpt (ihe hγ hδ) (ihn h₃ h₄) (ihs h₁ h₂)
  | @caseK γ δ e e' K n m b b' r r' _ _ _ ihe ihb ihr =>
    intro γ₂ δ₂ τ L hγ hδ
    obtain ⟨h₃, h₄⟩ := hyps_zero hρ hγ hδ
    simp only [Tm.dsubst, Flick.Tm.subst, spine_vars_binds]
    refine .caseK (ihe hγ hδ) (ihb (ghyp_lift (j := n) hγ (fun x hx => ?_) fun x hx => ?_)
      (dhyp_lift (j := m) hρ hδ (fun k hk => ?_) fun k hk => ?_)) (ihr h₃ h₄)
    · simp [aγ, hx]
    · simp [aγ, show ¬ x < n by omega]
    · simp [aδ, hk]; omega
    · simp [aδ, show ¬ k < m by omega]

end Flick.Elab
