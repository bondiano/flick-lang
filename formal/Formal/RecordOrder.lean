import Formal.Semantics

/-!
# Structural records and data values: no identity or observable field order

This module formalizes the core λFlick guarantees for issue #740:

1. **Order-independence of structural record operations**: two finite record
   spines (`IsRec`) with the same field map (`RecEquiv`) yield identical results
   under projection (`Tm.field`), equivalent records under update
   (`Tm.setField`), and identical bindings under record-spine pattern matching
   (`matchP` for `RecSpinePat`).
2. **No storage identity for data or records**: store allocation and inspection
   in `RHead` are restricted to `Ref` and `Lazy` cells (and handler/continuation
   markers); constructing records or constructors never allocates a store cell.
3. **Source-order elaboration to canonical records**: elaborating an out-of-order
   record literal via nested `.let_` bindings in source order (`elabSwap`)
   evaluates field expressions strictly in source order and reduces to the exact
   canonical `.rcons` value for any permutation of evaluated fields.
-/

namespace Flick

/-- A finite structural record spine: `.rnil` or `.rcons l e r`. -/
inductive IsRec : Tm → Prop
  | rnil : IsRec .rnil
  | rcons : IsRec r → IsRec (.rcons l e r)

/-- Two structural record spines are field-equivalent when they map every label
to the same term, regardless of the order in which `.rcons` lists those labels. -/
def RecEquiv (v₁ v₂ : Tm) : Prop :=
  IsRec v₁ ∧ IsRec v₂ ∧ ∀ l, v₁.field l = v₂.field l

theorem RecEquiv.refl {v : Tm} (h : IsRec v) : RecEquiv v v :=
  ⟨h, h, fun _ => rfl⟩

theorem RecEquiv.symm {v₁ v₂ : Tm} (h : RecEquiv v₁ v₂) : RecEquiv v₂ v₁ :=
  ⟨h.2.1, h.1, fun l => (h.2.2 l).symm⟩

theorem RecEquiv.trans {v₁ v₂ v₃ : Tm} (h₁ : RecEquiv v₁ v₂) (h₂ : RecEquiv v₂ v₃) :
    RecEquiv v₁ v₃ :=
  ⟨h₁.1, h₂.2.1, fun l => (h₁.2.2 l).trans (h₂.2.2 l)⟩

/-- Swapping two adjacent fields with distinct labels preserves `RecEquiv`. -/
theorem RecEquiv.rcons_swap {l₁ l₂ : Label} {e₁ e₂ r : Tm}
    (hne : l₁ ≠ l₂) (hr : IsRec r) :
    RecEquiv (.rcons l₁ e₁ (.rcons l₂ e₂ r)) (.rcons l₂ e₂ (.rcons l₁ e₁ r)) := by
  refine ⟨.rcons (.rcons hr), .rcons (.rcons hr), fun l => ?_⟩
  simp only [Tm.field]
  split
  · rename_i h₁
    subst h₁
    simp [hne.symm]
  · rename_i h₁
    split <;> simp

/-- Projection (`PHead.proj`) is independent of record label order. -/
theorem RecEquiv.field {v₁ v₂ : Tm} (h : RecEquiv v₁ v₂) (l : Label) :
    v₁.field l = v₂.field l :=
  h.2.2 l

theorem IsRec.setField_isRec {v v' : Tm} {l : Label} {w : Tm}
    (hv : IsRec v) (hs : v.setField l w = some v') : IsRec v' := by
  induction hv generalizing v' with
  | rnil => cases hs
  | @rcons r k e hr ih =>
    simp only [Tm.setField] at hs
    split at hs
    · cases hs; exact .rcons hr
    · cases h : r.setField l w with
      | none => simp [h] at hs
      | some r' =>
        simp [h] at hs
        cases hs
        exact .rcons (ih h)

theorem IsRec.setField_field {v v' : Tm} {l : Label} {w : Tm}
    (hv : IsRec v) (hs : v.setField l w = some v') (l' : Label) :
    v'.field l' = if l = l' then some w else v.field l' := by
  induction hv generalizing v' with
  | rnil => cases hs
  | @rcons r k e hr ih =>
    simp only [Tm.setField] at hs
    split at hs
    · rename_i hkl
      cases hs
      subst hkl
      simp only [Tm.field]
      split <;> rfl
    · rename_i hkl
      cases h : r.setField l w with
      | none => simp [h] at hs
      | some r' =>
        simp [h] at hs
        cases hs
        simp only [Tm.field]
        split
        · rename_i hkl'
          subst hkl'
          simp [Ne.symm hkl]
        · exact ih h

theorem IsRec.setField_isSome_iff {v : Tm} {l : Label} {w : Tm} (hv : IsRec v) :
    (v.setField l w).isSome = (v.field l).isSome := by
  induction hv with
  | rnil => rfl
  | @rcons r k e hr ih =>
    simp only [Tm.setField, Tm.field]
    split
    · rfl
    · cases h : r.setField l w <;> simp [← ih, h]

/-- Record update (`PHead.update`) is defined on the same labels and preserves
`RecEquiv`, independent of label order. -/
theorem RecEquiv.setField_isSome {v₁ v₂ : Tm} (h : RecEquiv v₁ v₂) (l : Label) (w : Tm) :
    (v₁.setField l w).isSome = (v₂.setField l w).isSome := by
  rw [h.1.setField_isSome_iff, h.2.1.setField_isSome_iff, h.2.2 l]

theorem RecEquiv.setField {v₁ v₂ v₁' v₂' : Tm} {l : Label} {w : Tm}
    (h : RecEquiv v₁ v₂) (h₁ : v₁.setField l w = some v₁') (h₂ : v₂.setField l w = some v₂') :
    RecEquiv v₁' v₂' := by
  refine ⟨h.1.setField_isRec h₁, h.2.1.setField_isRec h₂, fun l' => ?_⟩
  rw [h.1.setField_field h₁ l', h.2.1.setField_field h₂ l', h.2.2 l']

/-- A structural record pattern inspecting named fields without binding the
record spine itself at the leaves. -/
inductive RecSpinePat : Pat → Prop
  | rany : RecSpinePat .rany
  | rfield : RecSpinePat r → RecSpinePat (.rfield l p r)

theorem IsRec.matchP_rany {v : Tm} (hv : IsRec v) : matchP .rany v = some [] := by
  cases hv <;> rfl

/-- Record-spine pattern matching (`matchP`) is independent of record label order. -/
theorem RecEquiv.matchP {v₁ v₂ : Tm} {p : Pat} (h : RecEquiv v₁ v₂) (hp : RecSpinePat p) :
    matchP p v₁ = matchP p v₂ := by
  induction hp with
  | rany => rw [h.1.matchP_rany, h.2.1.matchP_rany]
  | @rfield r l q hr ih =>
    simp only [Flick.matchP, h.2.2 l, ih]

/-- No `RHead` transition inspects a record spine as a store cell or allocates
a store entry when building a record or constructor value: store growth occurs
only for `Ref.make`, `lazy`, `hdl`, and `performCtl` (covered in `Step`). -/
theorem RHead.store_grows_only_for_cells_or_hdl {Λ Λ' : List Rgn} {μ μ' : List Tm} {e e' : Tm}
    (h : RHead Λ μ e Λ' μ' e') (hgrow : μ.length < μ'.length) :
    (∃ v, e = .app (.const .refMake) v) ∨ (∃ body, e = .lazy body) ∨
      (∃ E al r body, e = .hdl E al r body) := by
  cases h with
  | refMake => exact .inl ⟨_, rfl⟩
  | lazy => exact .inr (.inl ⟨_, rfl⟩)
  | hdl => exact .inr (.inr ⟨_, _, _, _, rfl⟩)
  | _ => simp at hgrow

/-!
## Source-order elaboration into canonical records

When a record literal `{ l₁ = e₁; l₂ = e₂ }` is written out of canonical order
(`l₂ < l₁`), binding each field in source order via `.let_` and assembling the
record in canonical order preserves left-to-right evaluation and produces the
exact same canonical `.rcons` value once the fields evaluate.
-/

private theorem liftS_liftR_id (n : Nat) {ξ : Nat → Nat} {σ : Nat → Tm}
    (h : ∀ i, σ (ξ i) = .var i) (i : Nat) :
    liftS n σ (liftR n ξ i) = .var i := by
  unfold liftR liftS
  split
  · simp
  · rename_i hi
    have hge : ¬ (ξ (i - n) + n < n) := by omega
    simp only [hge, ↓reduceIte, Nat.add_sub_cancel, h, Tm.rename]
    congr 1; omega

/-- Substituting with a left inverse of a renaming recovers the original term. -/
theorem Tm.subst_rename_id (t : Tm) {ξ : Nat → Nat} {σ : Nat → Tm}
    (h : ∀ i, σ (ξ i) = .var i) :
    (t.rename ξ).subst σ = t := by
  induction t generalizing ξ σ with
  | var i => exact h i
  | lam b ih => simp only [Tm.rename, Tm.subst, ih (liftS_liftR_id 1 h)]
  | let_ e b ihe ihb => simp only [Tm.rename, Tm.subst, ihe h, ihb (liftS_liftR_id 1 h)]
  | letrec f b ihf ihb =>
    simp only [Tm.rename, Tm.subst, ihf (liftS_liftR_id 2 h), ihb (liftS_liftR_id 1 h)]
  | fix f ih => simp only [Tm.rename, Tm.subst, ih (liftS_liftR_id 2 h)]
  | acons p b r ihb ihr =>
    simp only [Tm.rename, Tm.subst, ihb (liftS_liftR_id p.binds h), ihr h]
  | aguard p g b r ihg ihb ihr =>
    simp only [Tm.rename, Tm.subst, ihg (liftS_liftR_id p.binds h),
      ihb (liftS_liftR_id p.binds h), ihr h]
  | scope e ih => simp only [Tm.rename, Tm.subst, ih (liftS_liftR_id 1 h)]
  | _ => simp [Tm.rename, Tm.subst, *]

/-- Shifting a term up by one binder and immediately substituting `v` for index 0
is the identity on every term (open or closed). -/
theorem Tm.subst_rename_shift (t v : Tm) : (t.rename (· + 1)).subst (inst [v]) = t :=
  t.subst_rename_id fun i => by simp [inst]

/-- Source-order `.let_` elaboration of a two-field record `{ l₁ = e₁; l₂ = e₂ }`
into canonical `.rcons l₂ _ (.rcons l₁ _ .rnil)` order. -/
def elabSwap (l₁ l₂ : Label) (e₁ e₂ : Tm) : Tm :=
  .let_ e₁ (.let_ (e₂.rename (· + 1)) (.rcons l₂ (.var 0) (.rcons l₁ (.var 1) .rnil)))

/-- Once `e₁` has evaluated to a value `v₁`, `elabSwap l₁ l₂ v₁ e₂` steps in one
pure `let` step so that `e₂` is evaluated next before constructing the record. -/
theorem elabSwap_step_after_first {l₁ l₂ : Label} {v₁ e₂ : Tm} (hv₁ : Value v₁) :
    PHead (elabSwap l₁ l₂ v₁ e₂)
      (.let_ e₂ (.rcons l₂ (.var 0) (.rcons l₁ (v₁.rename (· + 1)) .rnil))) := by
  have hstep := PHead.let_ (b := .let_ (e₂.rename (· + 1))
    (.rcons l₂ (.var 0) (.rcons l₁ (.var 1) .rnil))) hv₁
  simpa [elabSwap, Tm.subst, liftS, inst, Tm.subst_rename_shift] using hstep

/-- When both fields have evaluated to values `v₁`, `v₂`, `elabSwap l₁ l₂ v₁ v₂`
reduces in two pure steps to the exact canonical record `.rcons l₂ v₂ (.rcons l₁ v₁ .rnil)`. -/
theorem elabSwap_canonical {root : Roots} {Λ : List Rgn} {μ : List Tm}
    {l₁ l₂ : Label} {v₁ v₂ : Tm} (hv₁ : Value v₁) (hv₂ : Value v₂) :
    Steps root (· = .pure) Λ μ (elabSwap l₁ l₂ v₁ v₂)
      Λ μ (.rcons l₂ v₂ (.rcons l₁ v₁ .rnil)) := by
  have s1 : Step root .pure Λ μ (elabSwap l₁ l₂ v₁ v₂)
      (.cfg Λ μ (.let_ v₂ (.rcons l₂ (.var 0) (.rcons l₁ (v₁.rename (· + 1)) .rnil)))) :=
    Step.pure (E := []) (by intro F hF; cases hF) (elabSwap_step_after_first hv₁)
  have h2 : PHead (.let_ v₂ (.rcons l₂ (.var 0) (.rcons l₁ (v₁.rename (· + 1)) .rnil)))
      (.rcons l₂ v₂ (.rcons l₁ v₁ .rnil)) := by
    have h := PHead.let_ (b := .rcons l₂ (.var 0) (.rcons l₁ (v₁.rename (· + 1)) .rnil)) hv₂
    simpa [Tm.subst, inst, Tm.subst_rename_shift] using h
  have s2 : Step root .pure Λ μ
      (.let_ v₂ (.rcons l₂ (.var 0) (.rcons l₁ (v₁.rename (· + 1)) .rnil)))
      (.cfg Λ μ (.rcons l₂ v₂ (.rcons l₁ v₁ .rnil))) :=
    Step.pure (E := []) (by intro F hF; cases hF) h2
  exact .step s1 rfl (.step s2 rfl .refl)

end Flick
