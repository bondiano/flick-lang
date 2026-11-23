import Formal.Refine.Numbers

/-!
# Fixed-width integers and binary32 ordering (`docs/backends/numeric-policy.md`)

Formal specification and proofs for Flick's fixed-width numeric types (`Int8`,
`Int16`, `Int32`/`Int`, `Int64`, `UInt8`/`Byte`, `UInt16`, `UInt32`, `UInt64`,
and `Float32` ordering):

- Two's-complement modular wrapping for signed and unsigned widths `w ≥ 1`,
  including unary negation as the modular additive inverse on unsigned widths.
- Total `/` and `%`: division and remainder by zero yield `0`, and signed
  `min / -1 = min`, `min % -1 = 0`.
- Shift count masking modulo bit-width `w > 0`.
- Explicit checked, wrapping, and saturating conversions plus lossless widening
  (`Into` instances for signed→signed, unsigned→unsigned, and strictly wider
  unsigned→signed).
- Total preorder instances for `Formal.Refine.Numbers` (`fixedIntNum`) and
  total `Ord` with `NaN` greatest (`ordNaNLast`).

Like `Formal/Refine/Numbers.lean`, these theorems prove properties of the
specification model, not the Rust compiler or JS/Go runtime implementations.
-/

namespace Flick.FixedWidth

/-- Minimum representable integer for width `w` and signedness `signed`. -/
def minVal (w : Nat) (signed : Bool) : Int :=
  if signed then -((2 : Int) ^ (w - 1)) else 0

/-- Maximum representable integer for width `w` and signedness `signed`. -/
def maxVal (w : Nat) (signed : Bool) : Int :=
  if signed then (2 : Int) ^ (w - 1) - 1 else (2 : Int) ^ w - 1

/-- Representability predicate for a fixed-width integer of width `w`. -/
def inRange (w : Nat) (signed : Bool) (n : Int) : Prop :=
  minVal w signed ≤ n ∧ n ≤ maxVal w signed

private theorem pow2_pos (k : Nat) : (0 : Int) < (2 : Int) ^ k := by
  have h : (0 : Int) < ((2 ^ k : Nat) : Int) := by exact_mod_cast Nat.two_pow_pos k
  simpa using h

private theorem pow2_succ_eq {w : Nat} (hw : 1 ≤ w) :
    (2 : Int) ^ w = 2 * (2 : Int) ^ (w - 1) := by
  have hnat : 2 ^ w = 2 * 2 ^ (w - 1) := by
    conv => lhs; rw [show w = (w - 1) + 1 by omega]
    rw [Nat.pow_succ, Nat.mul_comm]
  have hint : ((2 ^ w : Nat) : Int) = ((2 * 2 ^ (w - 1) : Nat) : Int) := by exact_mod_cast hnat
  simpa using hint

private theorem pow2_le_pow2 {a b : Nat} (h : a ≤ b) :
    (2 : Int) ^ a ≤ (2 : Int) ^ b := by
  have hnat : 2 ^ a ≤ 2 ^ b := Nat.pow_le_pow_right (by omega) h
  have hint : ((2 ^ a : Nat) : Int) ≤ ((2 ^ b : Nat) : Int) := by exact_mod_cast hnat
  simpa using hint

theorem minVal_le_maxVal {w : Nat} (signed : Bool) :
    minVal w signed ≤ maxVal w signed := by
  have hp := pow2_pos (w - 1)
  have hpw := pow2_pos w
  cases signed <;> simp only [minVal, maxVal, ↓reduceIte, Bool.false_eq_true] <;> omega

/-- Two's-complement wrapping reduction into `[minVal w signed, maxVal w signed]`. -/
def wrap (w : Nat) (signed : Bool) (n : Int) : Int :=
  let m : Int := (2 : Int) ^ w
  let u := n % m
  if signed then
    let half : Int := (2 : Int) ^ (w - 1)
    if u < half then u else u - m
  else
    u

theorem wrap_inRange {w : Nat} (hw : 1 ≤ w) (signed : Bool) (n : Int) :
    inRange w signed (wrap w signed n) := by
  have hm : (0 : Int) < (2 : Int) ^ w := pow2_pos w
  have hlo : 0 ≤ n % (2 : Int) ^ w := Int.emod_nonneg n (by omega)
  have hhi : n % (2 : Int) ^ w < (2 : Int) ^ w := Int.emod_lt_of_pos n hm
  have h2 := pow2_succ_eq hw
  cases signed with
  | false =>
    simp only [inRange, minVal, maxVal, wrap, Bool.false_eq_true, ↓reduceIte]
    omega
  | true =>
    simp only [inRange, minVal, maxVal, wrap, ↓reduceIte]
    split <;> omega

theorem wrap_id {w : Nat} (hw : 1 ≤ w) (signed : Bool) {n : Int}
    (hr : inRange w signed n) : wrap w signed n = n := by
  have hm : (0 : Int) < (2 : Int) ^ w := pow2_pos w
  have h2 := pow2_succ_eq hw
  cases signed with
  | false =>
    simp only [inRange, minVal, maxVal, Bool.false_eq_true, ↓reduceIte] at hr
    simp only [wrap, Bool.false_eq_true, ↓reduceIte]
    exact Int.emod_eq_of_lt hr.1 (by omega)
  | true =>
    simp only [inRange, minVal, maxVal, ↓reduceIte] at hr
    simp only [wrap, ↓reduceIte]
    by_cases hn : 0 ≤ n
    · have hmod : n % (2 : Int) ^ w = n := Int.emod_eq_of_lt hn (by omega)
      rw [hmod]
      split <;> omega
    · have hadd : (n + (2 : Int) ^ w) % (2 : Int) ^ w = n + (2 : Int) ^ w :=
        Int.emod_eq_of_lt (by omega) (by omega)
      have hmod : n % (2 : Int) ^ w = n + (2 : Int) ^ w := by
        rw [← Int.add_emod_right n ((2 : Int) ^ w), hadd]
      rw [hmod]
      split <;> omega

theorem wrap_mod_equiv (w : Nat) (signed : Bool) (n : Int) :
    (wrap w signed n) % (2 : Int) ^ w = n % (2 : Int) ^ w := by
  cases signed with
  | false =>
    simp only [wrap, Bool.false_eq_true, ↓reduceIte]
    exact Int.emod_emod n _
  | true =>
    simp only [wrap, ↓reduceIte]
    split
    · exact Int.emod_emod n _
    · rw [Int.sub_emod_right, Int.emod_emod]

/-- Modular additive inverse holds for both signed and unsigned fixed-width integers. -/
theorem wrap_neg_add_cancel {w : Nat} (signed : Bool) (n : Int) :
    wrap w signed (wrap w signed (-n) + n) = 0 := by
  have hmod : (wrap w signed (-n) + n) % (2 : Int) ^ w = 0 := by
    rw [Int.add_emod, wrap_mod_equiv, ← Int.add_emod, Int.add_left_neg, Int.zero_emod]
  generalize wrap w signed (-n) = x at hmod ⊢
  have hp := pow2_pos (w - 1)
  cases signed with
  | false =>
    simp only [wrap, Bool.false_eq_true, ↓reduceIte]
    exact hmod
  | true =>
    simp only [wrap, ↓reduceIte]
    rw [hmod]
    simp [hp]

/-- Total wrapping division: division by zero returns `0`; signed `min / -1` wraps to `min`. -/
def safeDiv (w : Nat) (signed : Bool) (a b : Int) : Int :=
  if b = 0 then 0 else wrap w signed (Int.tdiv a b)

/-- Total wrapping remainder: remainder by zero returns `0`; signed `min % -1 = 0`. -/
def safeRem (w : Nat) (signed : Bool) (a b : Int) : Int :=
  if b = 0 then 0 else wrap w signed (Int.tmod a b)

theorem safeDiv_zero (w : Nat) (signed : Bool) (a : Int) :
    safeDiv w signed a 0 = 0 := by
  simp [safeDiv]

theorem safeRem_zero (w : Nat) (signed : Bool) (a : Int) :
    safeRem w signed a 0 = 0 := by
  simp [safeRem]

theorem safeDiv_inRange {w : Nat} (hw : 1 ≤ w) (signed : Bool) (a b : Int) :
    inRange w signed (safeDiv w signed a b) := by
  have hp := pow2_pos (w - 1)
  have hpw := pow2_pos w
  unfold safeDiv
  split
  · cases signed <;> simp only [inRange, minVal, maxVal, ↓reduceIte, Bool.false_eq_true] <;> omega
  · exact wrap_inRange hw signed _

theorem safeRem_inRange {w : Nat} (hw : 1 ≤ w) (signed : Bool) (a b : Int) :
    inRange w signed (safeRem w signed a b) := by
  have hp := pow2_pos (w - 1)
  have hpw := pow2_pos w
  unfold safeRem
  split
  · cases signed <;> simp only [inRange, minVal, maxVal, ↓reduceIte, Bool.false_eq_true] <;> omega
  · exact wrap_inRange hw signed _

theorem safeDiv_min_neg_one {w : Nat} (hw : 1 ≤ w) :
    safeDiv w true (minVal w true) (-1) = minVal w true := by
  have hp : (0 : Int) < (2 : Int) ^ (w - 1) := pow2_pos (w - 1)
  have h2 := pow2_succ_eq hw
  have htd : Int.tdiv (minVal w true) (-1) = (2 : Int) ^ (w - 1) := by
    simp [minVal, Int.tdiv_neg]
  have hmod : ((2 : Int) ^ (w - 1)) % ((2 : Int) ^ w) = (2 : Int) ^ (w - 1) :=
    Int.emod_eq_of_lt (by omega) (by omega)
  simp only [safeDiv, show (-1 : Int) ≠ 0 by omega, ↓reduceIte, htd, wrap, hmod]
  split
  · omega
  · simp only [minVal, ↓reduceIte]; omega

theorem safeRem_min_neg_one {w : Nat} :
    safeRem w true (minVal w true) (-1) = 0 := by
  have hp : (0 : Int) < (2 : Int) ^ (w - 1) := pow2_pos (w - 1)
  have htm : Int.tmod (minVal w true) (-1) = 0 := by
    simp [minVal, Int.tmod_neg]
  simp only [safeRem, show (-1 : Int) ≠ 0 by omega, ↓reduceIte, htm, wrap, Int.zero_emod]
  simp [hp]

/-- Checked division: rejects division by zero and signed `min / -1` overflow. -/
def checkedDiv (w : Nat) (signed : Bool) (a b : Int) : Option Int :=
  if b = 0 then
    none
  else
    let q := Int.tdiv a b
    if minVal w signed ≤ q ∧ q ≤ maxVal w signed then some q else none

theorem checkedDiv_sound (w : Nat) (signed : Bool) (a b q : Int) :
    checkedDiv w signed a b = some q ↔
      b ≠ 0 ∧ inRange w signed (Int.tdiv a b) ∧ q = Int.tdiv a b := by
  unfold checkedDiv inRange
  by_cases hb : b = 0
  · simp [hb]
  · simp only [hb, ↓reduceIte]
    by_cases hr : minVal w signed ≤ Int.tdiv a b ∧ Int.tdiv a b ≤ maxVal w signed
    · simp only [hr, ↓reduceIte, Option.some.injEq, true_and]
      exact ⟨fun h => ⟨hb, h.symm⟩, fun ⟨_, h⟩ => h.symm⟩
    · simp [hr]

theorem checkedDiv_min_neg_one_none (w : Nat) :
    checkedDiv w true (minVal w true) (-1) = none := by
  have htd : Int.tdiv (minVal w true) (-1) = (2 : Int) ^ (w - 1) := by
    simp [minVal, Int.tdiv_neg]
  have hnot : ¬ (minVal w true ≤ (2 : Int) ^ (w - 1) ∧ (2 : Int) ^ (w - 1) ≤ maxVal w true) := by
    simp only [minVal, maxVal, ↓reduceIte]
    omega
  simp only [checkedDiv, show (-1 : Int) ≠ 0 by omega, ↓reduceIte, htd]
  rw [if_neg hnot]

/-- Shift count masked modulo bit-width `w`. -/
def maskShift (w : Nat) (n : Int) : Nat :=
  (n % (w : Int)).toNat

theorem maskShift_lt {w : Nat} (hw : 0 < w) (n : Int) : maskShift w n < w := by
  have hwi : (0 : Int) < (w : Int) := by exact_mod_cast hw
  have hlo : 0 ≤ n % (w : Int) := Int.emod_nonneg n (by omega)
  have hhi : n % (w : Int) < (w : Int) := Int.emod_lt_of_pos n hwi
  unfold maskShift
  omega

theorem maskShift_periodic {w : Nat} (n : Int) :
    maskShift w (n + (w : Int)) = maskShift w n := by
  simp [maskShift, Int.add_emod_right]

/-- Explicit checked conversion: `some n` iff `n` fits without loss. -/
def checkedConv (w : Nat) (signed : Bool) (n : Int) : Option Int :=
  if minVal w signed ≤ n ∧ n ≤ maxVal w signed then some n else none

/-- Explicit saturating conversion: clamps out-of-range values to `[minVal, maxVal]`. -/
def saturatingConv (w : Nat) (signed : Bool) (n : Int) : Int :=
  if n < minVal w signed then minVal w signed
  else if maxVal w signed < n then maxVal w signed
  else n

theorem checkedConv_sound (w : Nat) (signed : Bool) (n m : Int) :
    checkedConv w signed n = some m ↔ inRange w signed n ∧ m = n := by
  unfold checkedConv inRange
  by_cases h : minVal w signed ≤ n ∧ n ≤ maxVal w signed
  · simp only [h, ↓reduceIte, Option.some.injEq, true_and]
    exact ⟨Eq.symm, Eq.symm⟩
  · simp [h]

theorem saturatingConv_inRange (w : Nat) (signed : Bool) (n : Int) :
    inRange w signed (saturatingConv w signed n) := by
  have hle := minVal_le_maxVal (w := w) signed
  unfold saturatingConv inRange
  split
  · exact ⟨Int.le_refl _, hle⟩
  · split
    · exact ⟨hle, Int.le_refl _⟩
    · omega

theorem saturatingConv_id (w : Nat) (signed : Bool) {n : Int}
    (hr : inRange w signed n) : saturatingConv w signed n = n := by
  unfold inRange at hr
  unfold saturatingConv
  split
  · omega
  · split
    · omega
    · rfl

theorem lossless_widening_signed {w₁ w₂ : Nat} (hle : w₁ ≤ w₂) {n : Int}
    (hr : inRange w₁ true n) : inRange w₂ true n := by
  have hpowi : (2 : Int) ^ (w₁ - 1) ≤ (2 : Int) ^ (w₂ - 1) := pow2_le_pow2 (by omega)
  simp only [inRange, minVal, maxVal, ↓reduceIte] at hr ⊢
  omega

theorem lossless_widening_unsigned {w₁ w₂ : Nat} (hle : w₁ ≤ w₂) {n : Int}
    (hr : inRange w₁ false n) : inRange w₂ false n := by
  have hpowi : (2 : Int) ^ w₁ ≤ (2 : Int) ^ w₂ := pow2_le_pow2 hle
  simp only [inRange, minVal, maxVal, Bool.false_eq_true, ↓reduceIte] at hr ⊢
  omega

theorem lossless_widening_unsigned_to_signed {w₁ w₂ : Nat} (hlt : w₁ < w₂) {n : Int}
    (hr : inRange w₁ false n) : inRange w₂ true n := by
  have hpowi : (2 : Int) ^ w₁ ≤ (2 : Int) ^ (w₂ - 1) := pow2_le_pow2 (by omega)
  have hpos := pow2_pos (w₂ - 1)
  simp only [inRange, minVal, maxVal, Bool.false_eq_true, ↓reduceIte] at hr ⊢
  omega

/-- Every lossless widening conversion agrees identically with wrapping conversion. -/
theorem lossless_wrap_eq {w₁ w₂ : Nat} {s₁ s₂ : Bool} (hw₂ : 1 ≤ w₂)
    (hsub : ∀ n, inRange w₁ s₁ n → inRange w₂ s₂ n) {n : Int} (hr : inRange w₁ s₁ n) :
    wrap w₂ s₂ n = n ∧ checkedConv w₂ s₂ n = some n ∧ saturatingConv w₂ s₂ n = n := by
  have hr₂ := hsub n hr
  refine ⟨wrap_id hw₂ s₂ hr₂, (checkedConv_sound w₂ s₂ n n).2 ⟨hr₂, rfl⟩,
    saturatingConv_id w₂ s₂ hr₂⟩

/-- Every fixed-width integer type forms a total preorder for `Formal.Refine.Numbers`. -/
def fixedIntNum (w : Nat) (signed : Bool) :
    Flick.Refine.Num { x : Int // inRange w signed x } where
  le a b := decide (a.val ≤ b.val)
  total a b := by simp; omega
  trans := by intro a b c; simp; omega

/-- Total ordering with `NaN` (`none`) greatest: models `Ord` on `Float` and `Float32`. -/
def ordNaNLast {V : Type} (N : Flick.Refine.Num V) : Flick.Refine.Num (Option V) where
  le a b :=
    match a, b with
    | _, none => true
    | none, some _ => false
    | some x, some y => N.le x y
  total a b := by
    cases a <;> cases b <;> simp
    exact N.total _ _
  trans := by
    intro a b c hab hbc
    cases a <;> cases b <;> cases c <;> simp_all
    exact N.trans hab hbc

theorem ordNaNLast_greatest {V : Type} (N : Flick.Refine.Num V) (x : Option V) :
    (ordNaNLast N).le x none = true := by
  cases x <;> rfl

end Flick.FixedWidth
