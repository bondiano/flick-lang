import Formal.Equiv.Mono

/-!
# Contextual equivalence: renaming and substitution of related terms

Related terms stay related under renaming (`Sim.rename`) and under related substitutions
(`Sim.subst`): two substitutions are related when their terms are, at either index, and the right
ones hold no run-time form (`SubR`), as the terms the steps substitute are values
(`Sim.value_noIns`). A rewrite is closed under both by its form; the states of the run of `born`
name only closed values (`bornS`, `bornV`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

theorem liftS_has {n : Nat} {σ : Nat → Tm} {i : Nat} {s : Site} (h : (liftS n σ i).Has s) :
    ∃ j, (σ j).Has s := by
  unfold liftS at h
  split at h
  · simp [Tm.Has] at h
  · exact ⟨_, (Tm.has_rename _ _ _).mp h⟩

/-- Substitution writes what the term writes and what the substituted terms write. -/
theorem Tm.has_subst_inv : ∀ (e : Tm) (σ : Nat → Tm) (s : Site), (e.subst σ).Has s →
    e.Has s ∨ ∃ i, (σ i).Has s := by
  intro e
  induction e <;> intro σ s h <;> simp only [Tm.subst, Tm.Has] at h ⊢ <;> grind [liftS_has]

theorem Tm.NoIns.subst {e : Tm} (h : e.NoIns) {σ : Nat → Tm} (hσ : ∀ i, (σ i).NoIns) :
    (e.subst σ).NoIns := fun h' => by
  rcases Tm.has_subst_inv _ _ _ h' with h' | ⟨i, h'⟩
  · exact h h'
  · exact hσ i h'

theorem _root_.Flick.Tm.Spine.rename : ∀ {t : Tm}, t.Spine → ∀ ξ : Nat → Nat, (t.rename ξ).Spine := by
  intro t
  induction t <;> intro h ξ <;> simp only [Tm.Spine, Tm.rename] at h ⊢
  all_goals first
    | trivial
    | exact h.elim
    | (apply_assumption; exact h)
    | (rcases h with h | ⟨hv, h⟩
       · exact .inl (by apply_assumption; exact h)
       · exact .inr ⟨hv.rename, by apply_assumption; exact h⟩)

theorem _root_.Flick.Tm.Spine.subst : ∀ {t : Tm}, t.Spine → ∀ σ : Nat → Tm, (t.subst σ).Spine := by
  intro t
  induction t <;> intro h σ <;> simp only [Tm.Spine, Tm.subst] at h ⊢
  all_goals first
    | trivial
    | exact h.elim
    | (apply_assumption; exact h)
    | (rcases h with h | ⟨hv, h⟩
       · exact .inl (by apply_assumption; exact h)
       · exact .inr ⟨hv.subst, by apply_assumption; exact h⟩)

theorem _root_.Flick.Tm.Spine.not_value : ∀ {t : Tm}, t.Spine → ¬ Value t := by
  intro t
  induction t with
  | app f a ihf iha =>
    intro h hv
    cases hv with
    | ctorApp hf _ he => rcases h with h | ⟨_, h⟩; exact ihf h hf; exact iha h he
    | constApp _ he => rcases h with h | ⟨_, h⟩; simp [Tm.Spine] at h; exact iha h he
  | rcons l e r ihe ihr =>
    intro h hv
    cases hv with
    | rcons h₁ h₂ => rcases h with h | ⟨_, h⟩; exact ihe h h₁; exact ihr h h₂
  | _ => intro h hv; simp only [Tm.Spine] at h <;> first | exact h.elim | (cases hv; done)

variable {W : World}

/-- A value on the right of related terms holds no run-time form. -/
theorem Sim.value_noIns {x : Pos} {a b : Tm} (h : Sim W x a b) : Value b → b.NoIns := by
  induction h with
  | lam _ hn => intro _; simpa [Tm.NoIns, Tm.Has] using hn
  | fix _ hn => intro _; simpa [Tm.NoIns, Tm.Has] using hn
  | task _ hn => intro _; simpa [Tm.NoIns, Tm.Has] using hn
  | drec _ hn => intro _; simpa [Tm.NoIns, Tm.Has] using hn
  | @app _ _ f' _ a' _ _ _ ih₁ ih₂ | @appK _ f' _ a' _ _ _ ih₁ ih₂ | @appV _ f' _ a' _ _ _ ih₁ ih₂ =>
    intro hv
    have : Value f' ∧ Value a' := by cases hv with
      | ctorApp hf _ he => exact ⟨hf, he⟩
      | constApp _ he => exact ⟨.const, he⟩
    have h₁ := ih₁ this.1; have h₂ := ih₂ this.2
    simp_all [Tm.NoIns, Tm.Has]
  | rcons _ _ _ ih₁ ih₂ | rconsK _ _ _ ih₁ ih₂ | rconsV _ _ _ ih₁ ih₂ =>
    intro hv
    cases hv with
    | rcons h₁ h₂ => have h₁ := ih₁ h₁; have h₂ := ih₂ h₂; simp_all [Tm.NoIns, Tm.Has]
  | dropA _ _ ih | dropB _ _ _ ih | forceA _ ih | jL _ _ _ ih | o4Bind _ _ ih => exact ih
  | bornV _ _ _ _ hn => intro _; exact hn
  | dead _ hn => intro _; exact hn
  | hloc => intro _; simp [Tm.NoIns, Tm.Has]
  | o4A | o4AllocR | o4Settle => intro hv; cases hv
  | o4B _ _ hf' _ _ _ _ hne hnb =>
    subst hf'; intro _; simp only [Tm.NoIns, Tm.Has]
    intro h; rcases h with h | h
    · exact Tm.NoIns.rename (· + 2) hne h
    · exact hnb h
  | o4Fix1 _ _ _ hf' _ _ _ _ hnb =>
    subst hf'; intro _; simp only [Tm.NoIns, Tm.Has]
    exact Tm.NoIns.subst hnb fun i => by cases i <;> simp [inst, Tm.NoIns, Tm.Has]
  | o4Fix2 _ _ _ hf' _ _ _ _ hne hnb =>
    subst hf'; intro _; simp only [Tm.NoIns, Tm.Has]
    intro h; rcases h with h | h
    · exact Tm.NoIns.rename (· + 2) hne h
    · exact hnb h
  | o4Alloc1 _ _ hs' _ _ _ _ hnc =>
    subst hs'; intro _
    exact Tm.NoIns.subst hnc fun i => by cases i <;> simp [inst, Tm.NoIns, Tm.Has]
  | _ => intro hv; first | (cases hv; done) | simp [Tm.NoIns, Tm.Has]

/-- A value is related to a value only by a congruence, so at any index. -/
theorem Sim.value_any {x : Pos} {a b : Tm} (h : Sim W x a b) (ha : Value a) (hb : Value b) :
    ∀ y, Sim W y a b := by
  induction h with
  | @app _ f f' a a' _ _ _ ih₁ ih₂ | @appK f f' a a' _ _ _ ih₁ ih₂ | @appV f f' a a' _ _ _ ih₁ ih₂ =>
    intro y
    have hfa : Value f ∧ Value a := by cases ha with
      | ctorApp hf _ he => exact ⟨hf, he⟩
      | constApp _ he => exact ⟨.const, he⟩
    have hfb : Value f' ∧ Value a' := by cases hb with
      | ctorApp hf _ he => exact ⟨hf, he⟩
      | constApp _ he => exact ⟨.const, he⟩
    by_cases hy : y = .park
    · subst hy; exact .appV hfa.1 (ih₁ hfa.1 hfb.1 _) (ih₂ hfa.2 hfb.2 _)
    · exact .app hy (ih₁ hfa.1 hfb.1 y) (ih₂ hfa.2 hfb.2 y)
  | rcons _ _ _ ih₁ ih₂ | rconsK _ _ _ ih₁ ih₂ | rconsV _ _ _ ih₁ ih₂ =>
    intro y
    cases ha with
    | rcons ha₁ ha₂ => cases hb with
      | rcons hb₁ hb₂ =>
        by_cases hy : y = .park
        · subst hy; exact .rconsV ha₁ (ih₁ ha₁ hb₁ _) (ih₂ ha₂ hb₂ _)
        · exact .rcons hy (ih₁ ha₁ hb₁ y) (ih₂ ha₂ hb₂ y)
  | dropA | dropB | forceA | jL | bornV | dead | bornA | bornB
  | o4B | o4Alloc1 | o4Bind | o4Settle => cases ha
  | bornS _ _ _ hs => rcases hs with rfl | rfl | rfl | rfl | rfl <;> cases ha
  | dropC | forceC | jR | o4A | o4AllocR => cases hb
  | hloc hB hH => intro _; exact .hloc hB hH
  | o4Fix1 hB hr hf hf' hcl hpe he hbd hnb _ _ => intro _; exact .o4Fix1 hB hr hf hf' hcl hpe he hbd hnb
  | o4Fix2 hB hl hf hf' hcl hpe he hbd hne hnb _ _ => intro _; exact .o4Fix2 hB hl hf hf' hcl hpe he hbd hne hnb
  | _ => intro y; first | (cases ha; done) | (constructor <;> assumption)

/-- Related substitutions: related terms at either index, variables or values on both sides, and
the right ones without a run-time form. -/
def SubR (W : World) (σ σ' : Nat → Tm) : Prop :=
  ∀ i, (∀ y, y ≠ .park → Sim W y (σ i) (σ' i)) ∧ (σ' i).NoIns ∧
    ((∃ j, σ i = .var j) ∨ Value (σ i)) ∧ ((∃ j, σ' i = .var j) ∨ Value (σ' i))

theorem liftR_succ (ξ : Nat → Nat) : (liftR 1 ξ) ∘ (· + 1) = (· + 1) ∘ ξ := by
  funext i; simp [liftR]

theorem rename_up (c : Tm) (ξ : Nat → Nat) :
    (c.rename (· + 1)).rename (liftR 1 ξ) = (c.rename ξ).rename (· + 1) := by
  rw [Tm.rename_rename, Tm.rename_rename, liftR_succ]

theorem subst_up (c : Tm) (σ : Nat → Tm) :
    (c.rename (· + 1)).subst (liftS 1 σ) = (c.subst σ).rename (· + 1) := by
  rw [Tm.subst_rename, Tm.rename_subst]
  congr 1

theorem BornSt.rename {l : Nat} {v t : Tm} (hc : v.Closed) (h : BornSt l v t) (ξ : Nat → Nat) :
    t.rename ξ = t := by
  rcases h with rfl | rfl | rfl | rfl | rfl <;> simp [Tm.rename, liftR, hc.rename]

theorem BornSt.subst {l : Nat} {v t : Tm} (hc : v.Closed) (h : BornSt l v t) (σ : Nat → Tm) :
    t.subst σ = t := by
  rcases h with rfl | rfl | rfl | rfl | rfl <;> simp [Tm.subst, Tm.rename, liftS, hc.subst]

theorem Sim.rename {x : Pos} {a b : Tm} (h : Sim W x a b) :
    ∀ ξ : Nat → Nat, Sim W x (a.rename ξ) (b.rename ξ) := by
  induction h with
  | dropA hb _ ih =>
    intro ξ; subst hb; simp only [Tm.rename]; exact .dropA (rename_up _ _) (ih ξ)
  | dropB hn hb _ ih =>
    intro ξ; subst hb; simp only [Tm.rename]; exact .dropB hn (rename_up _ _) (ih ξ)
  | dropC hb hn _ ih =>
    intro ξ; subst hb; simp only [Tm.rename]; exact .dropC (rename_up _ _) (Tm.NoIns.rename ξ hn) (ih ξ)
  | forceC hn _ ih => intro ξ; exact .forceC (Tm.NoIns.rename ξ hn) (ih ξ)
  | bornA hB hv hv' _ ih =>
    intro ξ; simp only [Tm.rename, liftR]; exact .bornA hB hv.rename hv'.rename (ih ξ)
  | bornB hB hv hv' _ ih =>
    intro ξ; simp only [Tm.rename, liftR]; exact .bornB hB hv.rename hv'.rename (ih ξ)
  | bornS hB hl hc hs =>
    intro ξ; rw [hs.rename hc]; exact .bornS hB hl hc hs
  | bornV hB hl hc hc' hn =>
    intro ξ; simp only [Tm.rename]; rw [hc.rename, hc'.rename]; exact .bornV hB hl hc hc' hn
  | dead hB hn => intro ξ; exact .dead hB (Tm.NoIns.rename ξ hn)
  | hloc hB hH => intro _; exact .hloc hB hH
  | o4A hB hf hbr hpe hpe' _ _ hne hnb ihe ihb =>
    subst hf hbr; intro ξ; simp only [Tm.rename]
    exact .o4A hB (by simp only [rename_up2]) (rot_rename _ _)
      (hpe.rename ξ) (hpe'.rename ξ) (ihe ξ) (ihb _) (Tm.NoIns.rename ξ hne) (Tm.NoIns.rename _ hnb)
  | o4B hB hbr hf' hpe hpe' _ _ hne hnb ihe ihb =>
    subst hbr hf'; intro ξ; simp only [Tm.rename]
    exact .o4B hB (rot_rename _ _) (by simp only [rename_up2])
      (hpe.rename ξ) (hpe'.rename ξ) (ihe ξ) (ihb _) (Tm.NoIns.rename ξ hne) (Tm.NoIns.rename _ hnb)
  | o4Fix1 hB hr hf hf' hcl hpe he _ hnb _ ihb =>
    subst hf hf'; intro ξ; simp only [Tm.rename]
    exact .o4Fix1 hB hr (by rw [rename_up2, hcl.rename]) (subst_loc_rename2 _ _ _)
      hcl hpe he (ihb _) (Tm.NoIns.rename _ hnb)
  | o4Fix2 hB hl hf hf' hcl hpe he _ hne hnb _ ihb =>
    subst hf hf'; intro ξ; simp only [Tm.rename]
    exact .o4Fix2 hB hl (subst_loc_rename2 _ _ _) (by rw [rename_up2, hcl.rename])
      hcl hpe he (ihb _) hne (Tm.NoIns.rename _ hnb)
  | o4Alloc1 hB hr hs' hcl hpe he _ hnc _ ihc =>
    subst hs'; intro ξ; simp only [Tm.rename, hcl.rename]
    exact .o4Alloc1 hB hr (subst_loc_rename _ _ _) hcl hpe he (ihc _) (Tm.NoIns.rename _ hnc)
  | o4AllocR hB hl hs hcl hpe he _ _ ihc =>
    subst hs; intro ξ; simp only [Tm.rename, hcl.rename]
    exact .o4AllocR hB hl (subst_loc_rename _ _ _) hcl hpe he (ihc _)
  | o4Bind hB _ ih =>
    intro ξ; simp only [Tm.rename]
    refine .o4Bind hB ?_
    rw [← subst_loc_rename]; exact ih ξ
  | o4Settle hB hH hhl hce hct hpt =>
    intro ξ; simp only [Tm.rename]; rw [hct.rename]
    exact .o4Settle hB hH hhl hce hct hpt
  | appK hs _ _ ih₁ ih₂ => intro ξ; exact .appK (hs.rename ξ) (ih₁ ξ) (ih₂ ξ)
  | appV hv _ _ ih₁ ih₂ => intro ξ; exact .appV hv.rename (ih₁ ξ) (ih₂ ξ)
  | rconsK hs _ _ ih₁ ih₂ => intro ξ; exact .rconsK (hs.rename ξ) (ih₁ ξ) (ih₂ ξ)
  | rconsV hv _ _ ih₁ ih₂ => intro ξ; exact .rconsV hv.rename (ih₁ ξ) (ih₂ ξ)
  | updK hs _ _ ih₁ ih₂ => intro ξ; exact .updK (hs.rename ξ) (ih₁ ξ) (ih₂ ξ)
  | updV hv _ _ ih₁ ih₂ => intro ξ; exact .updV hv.rename (ih₁ ξ) (ih₂ ξ)
  | aconsD hd hn _ ih =>
    intro ξ; simp only [Tm.rename]
    exact .aconsD (hd.imp (·.rename _) (·.rename _)) (Tm.NoIns.rename _ hn) (ih ξ)
  | _ =>
    intro ξ; simp only [Tm.rename]
    constructor <;> first | assumption | exact Tm.NoIns.rename _ ‹_› | apply_assumption

theorem SubR.lift {σ σ' : Nat → Tm} (h : SubR W σ σ') (n : Nat) :
    SubR W (liftS n σ) (liftS n σ') := by
  intro i
  unfold liftS
  split
  · exact ⟨fun _ hy => .var hy, by simp [Tm.NoIns, Tm.Has], .inl ⟨i, rfl⟩, .inl ⟨i, rfl⟩⟩
  · refine ⟨fun y hy => ((h _).1 y hy).rename _, Tm.NoIns.rename _ (h _).2.1, ?_, ?_⟩
    · rcases (h _).2.2.1 with ⟨j, hj⟩ | hv
      · rw [hj]; exact .inl ⟨j + n, rfl⟩
      · exact .inr hv.rename
    · rcases (h _).2.2.2 with ⟨j, hj⟩ | hv
      · rw [hj]; exact .inl ⟨j + n, rfl⟩
      · exact .inr hv.rename

theorem SubR.noIns {σ σ' : Nat → Tm} (h : SubR W σ σ') : ∀ i, (σ' i).NoIns := fun i => (h i).2.1

theorem SubR.leftVarVal {σ σ' : Nat → Tm} (h : SubR W σ σ') :
    ∀ i, (∃ j, σ i = .var j) ∨ Value (σ i) := fun i => (h i).2.2.1

theorem SubR.rightVarVal {σ σ' : Nat → Tm} (h : SubR W σ σ') :
    ∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i) := fun i => (h i).2.2.2

theorem Sim.subst {x : Pos} {a b : Tm} (h : Sim W x a b) :
    ∀ σ σ', SubR W σ σ' → Sim W x (a.subst σ) (b.subst σ') := by
  induction h with
  | var hx => intro σ σ' hσ; exact (hσ _).1 _ hx
  | dropA hb _ ih =>
    intro σ σ' hσ; subst hb; simp only [Tm.subst]; exact .dropA (subst_up _ _) (ih _ _ hσ)
  | dropB hn hb _ ih =>
    intro σ σ' hσ; subst hb; simp only [Tm.subst]; exact .dropB hn (subst_up _ _) (ih _ _ hσ)
  | dropC hb hn _ ih =>
    intro σ σ' hσ; subst hb; simp only [Tm.subst]
    exact .dropC (subst_up _ _) (Tm.NoIns.subst hn hσ.noIns) (ih _ _ hσ)
  | forceC hn _ ih => intro σ σ' hσ; exact .forceC (Tm.NoIns.subst hn hσ.noIns) (ih _ _ hσ)
  | bornA hB hv hv' _ ih =>
    intro σ σ' hσ; simp only [Tm.subst, liftS]; exact .bornA hB hv.subst hv'.subst (ih _ _ hσ)
  | bornB hB hv hv' _ ih =>
    intro σ σ' hσ; simp only [Tm.subst, liftS]; exact .bornB hB hv.subst hv'.subst (ih _ _ hσ)
  | bornS hB hl hc hs =>
    intro σ σ' _; rw [hs.subst hc]; exact .bornS hB hl hc hs
  | bornV hB hl hc hc' hn =>
    intro σ σ' _; simp only [Tm.subst]; rw [hc.subst, hc'.subst]; exact .bornV hB hl hc hc' hn
  | dead hB hn => intro σ σ' hσ; exact .dead hB (Tm.NoIns.subst hn hσ.noIns)
  | hloc hB hH => intro _ _ _; exact .hloc hB hH
  | o4A hB hf hbr hpe hpe' _ _ hne hnb ihe ihb =>
    subst hf hbr; intro σ σ' hσ; simp only [Tm.subst]
    exact .o4A hB (by simp only [subst_up2]) (rot_subst _ _)
      (hpe.subst hσ.leftVarVal) (hpe'.subst hσ.rightVarVal) (ihe _ _ hσ)
      (ihb _ _ ((hσ.lift 2).lift 1)) (Tm.NoIns.subst hne hσ.noIns)
      (Tm.NoIns.subst hnb ((hσ.lift 2).lift 1).noIns)
  | o4B hB hbr hf' hpe hpe' _ _ hne hnb ihe ihb =>
    subst hbr hf'; intro σ σ' hσ; simp only [Tm.subst]
    exact .o4B hB (rot_subst _ _) (by simp only [subst_up2])
      (hpe.subst hσ.leftVarVal) (hpe'.subst hσ.rightVarVal) (ihe _ _ hσ)
      (ihb _ _ ((hσ.lift 2).lift 1)) (Tm.NoIns.subst hne hσ.noIns)
      (Tm.NoIns.subst hnb ((hσ.lift 2).lift 1).noIns)
  | o4Fix1 hB hr hf hf' hcl hpe he _ hnb _ ihb =>
    subst hf hf'; intro σ σ' hσ; simp only [Tm.subst]
    exact .o4Fix1 hB hr (by simp only [subst_up2, hcl.subst]) (subst_loc_subst2 _ _ _)
      hcl hpe he (ihb _ _ ((hσ.lift 2).lift 1))
      (Tm.NoIns.subst hnb ((hσ.lift 2).lift 1).noIns)
  | o4Fix2 hB hl hf hf' hcl hpe he _ hne hnb _ ihb =>
    subst hf hf'; intro σ σ' hσ; simp only [Tm.subst]
    exact .o4Fix2 hB hl (subst_loc_subst2 _ _ _) (by simp only [subst_up2, hcl.subst])
      hcl hpe he (ihb _ _ ((hσ.lift 2).lift 1)) hne
      (Tm.NoIns.subst hnb ((hσ.lift 2).lift 1).noIns)
  | o4Alloc1 hB hr hs' hcl hpe he _ hnc _ ihc =>
    subst hs'; intro σ σ' hσ; simp only [Tm.subst, hcl.subst]
    exact .o4Alloc1 hB hr (subst_loc_subst _ _ _) hcl hpe he (ihc _ _ (hσ.lift 1))
      (Tm.NoIns.subst hnc (hσ.lift 1).noIns)
  | o4AllocR hB hl hs hcl hpe he _ _ ihc =>
    subst hs; intro σ σ' hσ; simp only [Tm.subst, hcl.subst]
    exact .o4AllocR hB hl (subst_loc_subst _ _ _) hcl hpe he (ihc _ _ (hσ.lift 1))
  | o4Bind hB _ ih =>
    intro σ σ' hσ; simp only [Tm.subst]
    refine .o4Bind hB ?_
    rw [← subst_loc_subst]; exact ih _ _ hσ
  | o4Settle hB hH hhl hce hct hpt =>
    intro σ σ' _; simp only [Tm.subst]; rw [hct.subst]
    exact .o4Settle hB hH hhl hce hct hpt
  | appK hs _ _ ih₁ ih₂ => intro σ σ' hσ; exact .appK (hs.subst σ) (ih₁ _ _ hσ) (ih₂ _ _ hσ)
  | appV hv _ _ ih₁ ih₂ => intro σ σ' hσ; exact .appV hv.subst (ih₁ _ _ hσ) (ih₂ _ _ hσ)
  | rconsK hs _ _ ih₁ ih₂ => intro σ σ' hσ; exact .rconsK (hs.subst σ) (ih₁ _ _ hσ) (ih₂ _ _ hσ)
  | rconsV hv _ _ ih₁ ih₂ => intro σ σ' hσ; exact .rconsV hv.subst (ih₁ _ _ hσ) (ih₂ _ _ hσ)
  | updK hs _ _ ih₁ ih₂ => intro σ σ' hσ; exact .updK (hs.subst σ) (ih₁ _ _ hσ) (ih₂ _ _ hσ)
  | updV hv _ _ ih₁ ih₂ => intro σ σ' hσ; exact .updV hv.subst (ih₁ _ _ hσ) (ih₂ _ _ hσ)
  | aconsD hd hn _ ih =>
    intro σ σ' hσ; simp only [Tm.subst]
    exact .aconsD (hd.imp (·.subst _) (·.subst _)) (Tm.NoIns.subst hn (hσ.lift _).noIns) (ih _ _ hσ)
  | _ =>
    intro σ σ' hσ; simp only [Tm.subst]
    constructor <;> first
      | assumption
      | (apply_assumption <;> first | exact hσ | exact hσ.lift _)
      | exact Tm.NoIns.subst ‹_› (hσ.lift _).noIns
      | exact Tm.NoIns.subst ‹_› hσ.noIns

end Flick.Equiv
