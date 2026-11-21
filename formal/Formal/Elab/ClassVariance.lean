import Formal.Elab.ClassVarianceBase

/-!
# Negative B1 checks, K1′ overlap checks, and end-to-end `SafeUse` (`Formal.Elab.ClassVariance`)
-/

namespace Flick.Elab

namespace ClassVarEx

private theorem tcon_ne_arr {T : Nat} {us : List Ty} {φ a b q} :
    Ty.tcon T us ≠ .arr φ a b q := by
  have hgo : ∀ (xs : List Ty) (acc : Ty), (∀ φ a b q, acc ≠ .arr φ a b q) →
      xs.foldl .app acc ≠ .arr φ a b q := by
    intro xs
    induction xs with
    | nil => intro acc h; exact h _ _ _ _
    | cons x xs ih => intro acc _; exact ih (.app acc x) fun _ _ _ _ h => by cases h
  exact hgo us (.con T) fun _ _ _ _ h => by cases h

private theorem tcon_inj {T₁ T₂ : Nat} {us₁ us₂ : List Ty}
    (h : Ty.tcon T₁ us₁ = Ty.tcon T₂ us₂) : T₁ = T₂ ∧ us₁ = us₂ := by
  have h1 := Ty.unspine_tcon T₁ us₁
  have h2 := Ty.unspine_tcon T₂ us₂
  rw [h] at h1; rw [h1] at h2; cases h2; exact ⟨rfl, rfl⟩

/-- Negative arrow subtyping: a runtime arrow is NOT a subtype of a pure arrow. -/
theorem not_tyLe_fnR_fnP : ¬ Ty.Le dvarEx fnR fnP := by
  intro h
  generalize ha : fnR = a at h
  generalize hb : fnP = b at h
  cases h with
  | refl => rw [← ha] at hb; cases hb
  | arr _ hst _ _ => cases ha; cases hb; exact hst
  | data _ _ _ _ _ _ _ => exact tcon_ne_arr ha.symm

/-- Negative `-`: wrong direction (`Show fnP` cannot serve `Show fnR`). -/
theorem not_serves_show_wrong_dir : ¬ Cn.Serves P_b1 ⟨cShow, [fnR]⟩ ⟨cShow, [fnP]⟩ := fun h =>
  not_tyLe_fnR_fnP (h.2.2.2.1 0 fnR fnP rfl rfl (by simp [P_b1, cvarEx, cShow])).1

/-- Negative `+`: wrong direction (`Default boxR` cannot serve `Default boxP`). -/
theorem not_serves_default_wrong_dir : ¬ Cn.Serves P_b1 ⟨cDefault, [boxP]⟩ ⟨cDefault, [boxR]⟩ := by
  intro h
  have hle : Ty.Le dvarEx boxR boxP := h.2.2.1 0 boxP boxR rfl rfl (by simp [P_b1, cvarEx, cDefault])
  generalize ha : boxR = a at hle
  generalize hb : boxP = b at hle
  cases hle with
  | refl => rw [← ha] at hb; obtain ⟨_, he⟩ := tcon_inj hb; cases he
  | arr => exact tcon_ne_arr ha
  | data _ _ _ _ hpos _ _ =>
    obtain ⟨rfl, rfl⟩ := tcon_inj ha.symm
    obtain ⟨_, rfl⟩ := tcon_inj hb.symm
    exact not_tyLe_fnR_fnP (hpos 0 fnR fnP rfl rfl (by simp [dvarEx]))

/-- Negative `inv`: `Monoid fnP` does NOT serve `Monoid fnR`. -/
theorem not_serves_monoid_sub : ¬ Cn.Serves P_b1 ⟨cMonoid, [fnR]⟩ ⟨cMonoid, [fnP]⟩ := by
  intro h
  have heq := h.2.2.2.2 0 fnR fnP rfl rfl (.inl (by simp [P_b1, cvarEx, cMonoid]))
  cases heq

/-- Negative `bi`: `TypeName Int` does NOT serve `TypeName Bool` (exact equality required). -/
theorem not_serves_typename_diff : ¬ Cn.Serves P_b1 ⟨cTypeName, [boolTy]⟩ ⟨cTypeName, [intTy]⟩ := by
  intro h
  have heq := h.2.2.2.2 0 boolTy intTy rfl rfl (.inr (by simp [P_b1, cvarEx, cTypeName]))
  cases heq

/-- Negative `-` reach-dropping: `Show (Unit →P^{s} Unit)` cannot serve `Show (Unit →P^∅ Unit)`,
because the head shows `s` while the query does not. -/
theorem not_serves_show_drop_reach (s : Nat) :
    ¬ Cn.Serves P_b1 ⟨cShow, [fnP]⟩ ⟨cShow, [fnPs s]⟩ := by
  intro h
  have hsh := (h.2.2.2.1 0 fnP (fnPs s) rfl rfl (by simp [P_b1, cvarEx, cShow])).2 s
    (by simp [fnPs, Ty.Shown])
  simp [fnP, Ty.Shown] at hsh

/-! ## 3. K1′ overlap checks -/

/-- Overlapping comparable `-` heads (`Show fnR` and `Show fnP`) violate K1′. -/
def P_overlap_neg : Prog :=
  { P_b1 with
    insts := fun
      | 0 => some ⟨cShow, [fnR], [], []⟩
      | 1 => some ⟨cShow, [fnP], [], []⟩
      | _ => none }

theorem not_k1'_comparable_neg : ¬ ∃ lt, Coherent P_overlap_neg lt := by
  rintro ⟨lt, hC⟩
  have h0 : Cn.Serves P_overlap_neg ⟨cShow, [fnP]⟩ ⟨cShow, [fnR.subst Ty.var]⟩ := by
    simpa [fnR, Ty.subst, Stage.subst] using serves_show_fnP
  have h1 : Cn.Serves P_overlap_neg ⟨cShow, [fnP]⟩ ⟨cShow, [fnP.subst Ty.var]⟩ := by
    simpa [fnP, Ty.subst, Stage.subst] using Cn.Serves.refl (P := P_overlap_neg) ⟨cShow, [fnP]⟩
  have heq := hC.k1' (I := 0) (J := 1) (σ := Ty.var) (σ' := Ty.var) rfl rfl h0 h1
  cases heq

/-- Mutually incomparable `-` product heads `Show (Pair fnR fnP)` and `Show (Pair fnP fnR)`:
neither head serves the other, yet BOTH serve the common query `Show (Pair fnP fnP)`, so K1′
rejects them. -/
def P_pair_neg : Prog :=
  { P_b1 with
    insts := fun
      | 0 => some ⟨cShow, [pairTy fnR fnP], [], []⟩
      | 1 => some ⟨cShow, [pairTy fnP fnR], [], []⟩
      | _ => none }

private theorem tyLe_pair {a₁ a₂ b₁ b₂ : Ty}
    (ha : Ty.Le dvarEx a₁ a₂) (hb : Ty.Le dvarEx b₁ b₂)
    (hsha : ∀ n, a₁.Shown n → a₂.Shown n) (hshb : ∀ n, b₁.Shown n → b₂.Shown n) :
    Ty.Le dvarEx (pairTy a₁ b₁) (pairTy a₂ b₂) := by
  refine .data (by decide) (by decide) rfl ?_ ?_ ?_ ?_
  · intro j u u' hu hu' n hn
    rcases j with _ | _ | j
    · simp at hu hu'; subst hu; subst hu'; exact hsha n hn
    · simp at hu hu'; subst hu; subst hu'; exact hshb n hn
    · simp at hu
  · intro j u u' hu hu' _
    rcases j with _ | _ | j
    · simp at hu hu'; subst hu; subst hu'; exact ha
    · simp at hu hu'; subst hu; subst hu'; exact hb
    · simp at hu
  · intro j u u' hu _ hd
    rcases j with _ | _ | j <;> simp [dvarEx] at hu hd
  · intro j u u' hu _ hd
    rcases j with _ | _ | j <;> simp [dvarEx] at hu hd

theorem pair_heads_incomparable_lr :
    ¬ Cn.Serves P_pair_neg ⟨cShow, [pairTy fnR fnP]⟩ ⟨cShow, [pairTy fnP fnR]⟩ := by
  intro h
  have hle : Ty.Le dvarEx (pairTy fnR fnP) (pairTy fnP fnR) :=
    (h.2.2.2.1 0 _ _ rfl rfl (by simp [P_pair_neg, P_b1, cvarEx, cShow])).1
  generalize ha : pairTy fnR fnP = a at hle
  generalize hb : pairTy fnP fnR = b at hle
  cases hle with
  | refl => rw [← ha] at hb; obtain ⟨_, he⟩ := tcon_inj hb; cases he
  | arr => exact tcon_ne_arr ha
  | data _ _ _ _ hpos _ _ =>
    obtain ⟨rfl, rfl⟩ := tcon_inj ha.symm
    obtain ⟨_, rfl⟩ := tcon_inj hb.symm
    exact not_tyLe_fnR_fnP (hpos 0 fnR fnP rfl rfl (by simp [dvarEx]))

theorem pair_heads_incomparable_rl :
    ¬ Cn.Serves P_pair_neg ⟨cShow, [pairTy fnP fnR]⟩ ⟨cShow, [pairTy fnR fnP]⟩ := by
  intro h
  have hle : Ty.Le dvarEx (pairTy fnP fnR) (pairTy fnR fnP) :=
    (h.2.2.2.1 0 _ _ rfl rfl (by simp [P_pair_neg, P_b1, cvarEx, cShow])).1
  generalize ha : pairTy fnP fnR = a at hle
  generalize hb : pairTy fnR fnP = b at hle
  cases hle with
  | refl => rw [← ha] at hb; obtain ⟨_, he⟩ := tcon_inj hb; cases he
  | arr => exact tcon_ne_arr ha
  | data _ _ _ _ hpos _ _ =>
    obtain ⟨rfl, rfl⟩ := tcon_inj ha.symm
    obtain ⟨_, rfl⟩ := tcon_inj hb.symm
    exact not_tyLe_fnR_fnP (hpos 1 fnR fnP rfl rfl (by simp [dvarEx]))

theorem not_k1'_incomparable_pair_neg : ¬ ∃ lt, Coherent P_pair_neg lt := by
  rintro ⟨lt, hC⟩
  have hmk : ∀ a b, Ty.Le dvarEx (pairTy fnP fnP) (pairTy a b) →
      (∀ n, (pairTy a b).Shown n → (pairTy fnP fnP).Shown n) →
      Cn.Serves P_pair_neg ⟨cShow, [pairTy fnP fnP]⟩ ⟨cShow, [pairTy a b]⟩ := by
    intro a b hle hsh
    refine ⟨rfl, rfl, ?_, ?_, ?_⟩
    · intro j _ _ hq _ hv
      cases j with | zero => simp [P_pair_neg, P_b1, cvarEx, cShow] at hv | succ _ => simp at hq
    · intro j q h hq hh _
      cases j with
      | zero => simp at hq hh; subst hq; subst hh; exact ⟨hle, hsh⟩
      | succ _ => simp at hq
    · intro j _ _ hq _ hv
      cases j with
      | zero => rcases hv with hv | hv <;> simp [P_pair_neg, P_b1, cvarEx, cShow] at hv
      | succ _ => simp at hq
  have h0 : Cn.Serves P_pair_neg ⟨cShow, [pairTy fnP fnP]⟩
      ⟨cShow, [(pairTy fnR fnP).subst Ty.var]⟩ := by
    rw [Ty.subst_id]
    exact hmk fnR fnP (tyLe_pair tyLe_fnP_fnR .refl (fun _ h => h) (fun _ h => h))
      fun _ h => by simp [pairTy, Ty.tcon, fnR, fnP, Ty.Shown] at h
  have h1 : Cn.Serves P_pair_neg ⟨cShow, [pairTy fnP fnP]⟩
      ⟨cShow, [(pairTy fnP fnR).subst Ty.var]⟩ := by
    rw [Ty.subst_id]
    exact hmk fnP fnR (tyLe_pair .refl tyLe_fnP_fnR (fun _ h => h) (fun _ h => h))
      fun _ h => by simp [pairTy, Ty.tcon, fnR, fnP, Ty.Shown] at h
  have heq := hC.k1' (I := 0) (J := 1) (σ := Ty.var) (σ' := Ty.var) rfl rfl h0 h1
  cases heq

/-- Overlapping comparable `+` heads (`Default boxP` and `Default boxR`) violate K1′. -/
def P_overlap_pos : Prog :=
  { P_b1 with
    insts := fun
      | 0 => some ⟨cDefault, [boxP], [], []⟩
      | 1 => some ⟨cDefault, [boxR], [], []⟩
      | _ => none }

theorem not_k1'_comparable_pos : ¬ ∃ lt, Coherent P_overlap_pos lt := by
  rintro ⟨lt, hC⟩
  have h0 : Cn.Serves P_overlap_pos ⟨cDefault, [boxR]⟩ ⟨cDefault, [boxP.subst Ty.var]⟩ := by
    rw [Ty.subst_id]; exact serves_default_boxR
  have h1 : Cn.Serves P_overlap_pos ⟨cDefault, [boxR]⟩ ⟨cDefault, [boxR.subst Ty.var]⟩ := by
    rw [Ty.subst_id]; exact Cn.Serves.refl _
  have heq := hC.k1' (I := 0) (J := 1) (σ := Ty.var) (σ' := Ty.var) rfl rfl h0 h1
  cases heq

/-- Distinct `inv` (`Monoid fnP` / `Monoid fnR`) and `bi` (`TypeName Int` / `TypeName Bool`)
heads satisfy K1′: no common query exists. -/
theorem k1'_monoid_distinct {Q : Cn} {σ σ' : Nat → Ty}
    (h1 : Cn.Serves P_b1 Q ⟨cMonoid, [fnP.subst σ]⟩)
    (h2 : Cn.Serves P_b1 Q ⟨cMonoid, [fnR.subst σ']⟩) : False := by
  have hinv : ∀ j, P_b1.clsVar Q.cls j = .inv ∨ P_b1.clsVar Q.cls j = .bi := fun j => by
    have hc : cMonoid = Q.cls := h1.1
    rw [← hc]; exact .inl (by simp [P_b1, cvarEx, cMonoid])
  have e1 := Cn.Serves.eq_of_inv hinv h1
  have e2 := Cn.Serves.eq_of_inv hinv h2
  rw [e1] at e2
  simp [fnP, fnR, Ty.subst, Stage.subst] at e2

theorem k1'_typename_distinct {Q : Cn} {σ σ' : Nat → Ty}
    (h1 : Cn.Serves P_b1 Q ⟨cTypeName, [intTy.subst σ]⟩)
    (h2 : Cn.Serves P_b1 Q ⟨cTypeName, [boolTy.subst σ']⟩) : False := by
  have hinv : ∀ j, P_b1.clsVar Q.cls j = .inv ∨ P_b1.clsVar Q.cls j = .bi := fun j => by
    have hc : cTypeName = Q.cls := h1.1
    rw [← hc]
    cases j with
    | zero => exact .inr rfl
    | succ _ => exact .inl rfl
  have e1 := Cn.Serves.eq_of_inv hinv h1
  have e2 := Cn.Serves.eq_of_inv hinv h2
  rw [e1] at e2
  simp [intTy, boolTy, Ty.subst] at e2

end ClassVarEx

/-! ## 4. End-to-end `SafeUse` under B1 elaboration -/

/-- **B1 dictionary transport retains `SafeUse` (T4′)**: every closed source program accepted
by `Elab` (including variance-directed instance resolution `Ent.inst` / `dict_transport`,
lowered to core `LeLive.dict`) translates to a core term satisfying `SafeUse`. -/
theorem T6a_safeUse {P : Prog} {Sg : Sig} {N : Names} {root : Roots}
    (hR : Real P Sg N) (hroot : RootTy Sg root) :
    SafeUse root (fun e'' t φ =>
      ∃ e τ φ₀ e', Elab P [] [] e τ φ₀ e' ∧ Tr N id id e' e'' ∧
        t = trT N τ ∧ φ = trStage N φ₀) :=
  (T4' hroot).mono fun _ _ _ ⟨_, _, _, _, he, htr, ht, hφ⟩ => by
    subst ht; subst hφ
    obtain ⟨e''', htr', htyp⟩ := T6a_closed hR he [] Facts.none
    rw [htr.eq, ← htr'.eq]
    exact htyp

end Flick.Elab
