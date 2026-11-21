import Formal.Elab.Variance.InferDef

/-!
# Soundness of `inferTyVar` (`Formal.Elab.Variance.InferSound`)
-/

namespace Flick.Elab

/-- **Soundness of computable type-variance inference**: if `inferTyVar dvar i q t ⊑ v` and
`v ≠ inv`, then `v = bi` implies `i` does not occur in `t`, and `v ∈ {pos, neg}` implies
`Ty.Pol dvar i v q t`. -/
theorem inferTyVar_sound {dvar : Nat → Nat → Variance} {i : Nat} {q : Bool} {t : Ty}
    {v : Variance} (hne : v ≠ .inv) (hle : (inferTyVar dvar i q t).Le v) :
    (v = .bi → ¬ t.Occurs i) ∧ ((v = .pos ∨ v = .neg) → Ty.Pol dvar i v q t) := by
  refine ⟨fun hbi => inferTyVar_bi (Variance.le_bi (hbi ▸ hle)), fun hpn => ?_⟩
  by_cases ho : t.occursB i = true
  · unfold inferTyVar at hle
    simp only [ho, Bool.not_true, Bool.false_eq_true, ↓reduceIte] at hle
    cases ht : t with
    | var m =>
      rw [ht] at ho hle
      have hm : m = i := by simpa [Ty.occursB] using ho
      subst hm
      simp only [beq_self_eq_true, ↓reduceIte] at hle
      refine .var ?_
      cases q <;> cases v <;> simp [Variance.Le, Variance.at] at hle hpn ⊢
    | con _ | cnil => rw [ht] at ho; simp [Ty.occursB] at ho
    | arr φ a b c =>
      have hlt_a : sizeOf a < sizeOf t := by rw [ht]; simp; omega
      have hlt_b : sizeOf b < sizeOf t := by rw [ht]; simp; omega
      rw [ht] at hle; dsimp only at hle
      have h1 := Variance.le_join_left hle
      have h2 := Variance.le_join_left (Variance.le_join_right hle)
      have h3 := Variance.le_join_left (Variance.le_join_right (Variance.le_join_right hle))
      have h4 := Variance.le_join_right (Variance.le_join_right (Variance.le_join_right hle))
      have hφ : ¬ φ.Occurs i := by
        intro hx; have hb := (Stage.occursB_iff i φ).2 hx
        simp only [hb, ↓reduceIte] at h1
        cases v <;> simp [Variance.Le] at h1 hne
      have hpa : v = .pos → ¬ a.Occurs i := by
        rintro rfl
        cases q with
        | false =>
          intro hx; have hb := (Ty.occursB_iff i a).2 hx; simp [hb, Variance.Le] at h2
        | true =>
          simp only [Bool.not_true, Bool.false_eq_true, ↓reduceIte] at h2
          rcases Variance.le_pos_of h2 with hbi | hpos
          · exact inferTyVar_bi hbi
          · exact ((inferTyVar_sound (v := .pos) (by decide)
              (by rw [hpos]; exact Variance.le_refl .pos)).2 (.inl rfl)).noOccurs (.inr ⟨rfl, rfl⟩)
      have hPa : Ty.Pol dvar i v (!q) a := by
        cases q with
        | false =>
          by_cases ha : a.occursB i = true
          · simp [ha] at h2
            cases v <;> simp [Variance.Le] at h2 hne
          · exact .none fun hx => ha ((Ty.occursB_iff i a).2 hx)
        | true =>
          simp only [Bool.not_true, Bool.false_eq_true, ↓reduceIte] at h2
          exact (inferTyVar_sound hne h2).2 hpn
      have hPb : Ty.Pol dvar i v q b := (inferTyVar_sound hne h3).2 hpn
      have hc : ¬ c.Occurs i ∨ v.at q := by
        by_cases hcb : c.occursB i = true
        · right
          simp only [hcb, ↓reduceIte] at h4
          cases q <;> cases v <;> simp [Variance.Le, Variance.at] at h4 hpn ⊢
        · left; exact fun hx => hcb ((Ty.occursB_iff i c).2 hx)
      exact .arr hφ hpa hPa hPb hc
    | app f u =>
      rw [ht] at hle; dsimp only at hle
      cases hus : (Ty.app f u).unspine with
      | none =>
        rw [hus] at hle
        cases v <;> simp [Variance.Le] at hle hne
      | some p =>
        obtain ⟨T, us⟩ := p
        rw [hus] at hle
        by_cases h24 : (T == 2 || T == 4) = true
        · simp only [h24, ↓reduceIte] at hle
          cases v <;> simp [Variance.Le] at hle hne
        · simp only [h24, Bool.false_eq_true, ↓reduceIte] at hle
          have hT2 : T ≠ 2 := by intro e; subst e; simp at h24
          have hT4 : T ≠ 4 := by intro e; subst e; simp at h24
          have heq := Ty.unspine_eq hus
          have hgo : ∀ (k : Nat) (xs : List Ty),
              (inferTyVarArgs dvar i q T k xs).Le v →
              (∀ (m : Nat) (x : Ty), xs[m]? = some x → sizeOf x < sizeOf t) →
              (∀ (m : Nat) (x : Ty), xs[m]? = some x → dvar T (k + m) = .pos → Ty.Pol dvar i v q x) ∧
              (∀ (m : Nat) (x : Ty), xs[m]? = some x → dvar T (k + m) = .neg → v = .pos → ¬ x.Occurs i) ∧
              (∀ (m : Nat) (x : Ty), xs[m]? = some x → dvar T (k + m) = .neg → Ty.Pol dvar i v (!q) x) ∧
              (∀ (m : Nat) (x : Ty), xs[m]? = some x → dvar T (k + m) = .inv → ¬ x.Occurs i) ∧
              (∀ (m : Nat) (x : Ty), xs[m]? = some x → dvar T (k + m) = .bi → ¬ v.at q → ¬ x.Occurs i) := by
            intro k xs
            induction xs generalizing k with
            | nil =>
              intro _ _
              refine ⟨fun _ _ hm => by simp at hm, fun _ _ hm => by simp at hm,
                fun _ _ hm => by simp at hm, fun _ _ hm => by simp at hm,
                fun _ _ hm => by simp at hm⟩
            | cons z zs ih =>
              intro hg hsz
              unfold inferTyVarArgs at hg
              have hz := Variance.le_join_left hg
              have hzs := Variance.le_join_right hg
              have ih' := ih (k + 1) hzs fun m x hm => hsz (m + 1) x (by simpa using hm)
              have hsz0 : sizeOf z < sizeOf t := hsz 0 z rfl
              refine ⟨?_, ?_, ?_, ?_, ?_⟩
              · intro m x hm hd
                cases m with
                | zero =>
                  simp at hm; subst hm; rw [Nat.add_zero] at hd; rw [hd] at hz
                  exact (inferTyVar_sound hne hz).2 hpn
                | succ m =>
                  rw [show k + (m + 1) = k + 1 + m by omega] at hd
                  exact ih'.1 m x (by simpa using hm) hd
              · intro m x hm hd hv
                cases m with
                | zero =>
                  simp at hm; subst hm; rw [Nat.add_zero] at hd; rw [hd] at hz
                  subst hv
                  cases q with
                  | false =>
                    intro hx; have hb := (Ty.occursB_iff i z).2 hx; simp [hb, Variance.Le] at hz
                  | true =>
                    simp only [Bool.not_true, Bool.false_eq_true, ↓reduceIte] at hz
                    rcases Variance.le_pos_of hz with hbi | hpos
                    · exact inferTyVar_bi hbi
                    · exact ((inferTyVar_sound (v := .pos) (by decide)
                        (by rw [hpos]; exact Variance.le_refl .pos)).2 (.inl rfl)).noOccurs (.inr ⟨rfl, rfl⟩)
                | succ m =>
                  rw [show k + (m + 1) = k + 1 + m by omega] at hd
                  exact ih'.2.1 m x (by simpa using hm) hd hv
              · intro m x hm hd
                cases m with
                | zero =>
                  simp at hm; subst hm; rw [Nat.add_zero] at hd; rw [hd] at hz
                  cases q with
                  | false =>
                    by_cases hzb : z.occursB i = true
                    · simp only [hzb, ↓reduceIte] at hz
                      cases v <;> simp [Variance.Le] at hz hne
                    · exact .none fun hx => hzb ((Ty.occursB_iff i z).2 hx)
                  | true =>
                    simp only [Bool.not_true, Bool.false_eq_true, ↓reduceIte] at hz
                    exact (inferTyVar_sound hne hz).2 hpn
                | succ m =>
                  rw [show k + (m + 1) = k + 1 + m by omega] at hd
                  exact ih'.2.2.1 m x (by simpa using hm) hd
              · intro m x hm hd
                cases m with
                | zero =>
                  simp at hm; subst hm; rw [Nat.add_zero] at hd; rw [hd] at hz
                  intro hx; have hb := (Ty.occursB_iff i z).2 hx
                  simp only [hb, ↓reduceIte] at hz
                  cases v <;> simp [Variance.Le] at hz hne
                | succ m =>
                  rw [show k + (m + 1) = k + 1 + m by omega] at hd
                  exact ih'.2.2.2.1 m x (by simpa using hm) hd
              · intro m x hm hd hnat
                cases m with
                | zero =>
                  simp at hm; subst hm; rw [Nat.add_zero] at hd; rw [hd] at hz
                  intro hx; have hb := (Ty.occursB_iff i z).2 hx
                  simp only [hb, ↓reduceIte] at hz
                  cases q <;> cases v <;> simp [Variance.Le, Variance.at] at hz hnat hpn
                | succ m =>
                  rw [show k + (m + 1) = k + 1 + m by omega] at hd
                  exact ih'.2.2.2.2 m x (by simpa using hm) hd hnat
          have hall := hgo 0 us hle fun m x hm => by
            have := Ty.sizeOf_lt_tcon (T := T) hm; rw [← heq, ← ht] at this; exact this
          simp only [Nat.zero_add] at hall
          exact Ty.Pol.data hT2 hT4 hall.1 hall.2.1 hall.2.2.1 hall.2.2.2.1 hall.2.2.2.2 heq
    | cun _ _ | rnm _ _ | run _ _ | all _ _ =>
      rw [ht] at hle; cases v <;> simp [Variance.Le] at hle hne
  · exact .none fun hx => ho ((Ty.occursB_iff i t).2 hx)
termination_by sizeOf t
decreasing_by
  all_goals simp_wf
  all_goals first | exact hlt_a | exact hlt_b | exact hsz0

end Flick.Elab
