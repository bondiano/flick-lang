import Formal.Typing
import Formal.TypeEquivArgs

/-!
# Typing by use: the stage of a use and subtyping

A value is typed by use (`Rules.lean`): a region is needed live where a value is used, at
the stage `Stage.use φ C` of a call of an arrow qualified `C`, not where it is held.
-/

namespace Flick

/-- `Live C` uses the same row atoms as `Scope s`; no new row element is needed.
A pure computation does not use a region, even when its value holds one. -/
def Stage.use : Stage → Caps → Stage
  | .pure, _ => .pure
  | .rt r ε, C => .rt (Row.union r (Row.ofCaps C)) ε

/-- Subtyping compares the stages at which qualified values are used, so it cannot hide a
qualifier in an argument or a result type. -/
inductive LeLive (S : Sig) : Ty → Ty → Prop
  | refl : LeLive S t t
  /-- Equality of row terms is available in invariant positions too. -/
  | equiv : Ty.Equiv a b → LeLive S a b
  | arrow : LeLive S a' a →
      (Stage.use φ C).le (Stage.use φ' C') → LeLive S t t' →
      Caps.Sub (Ty.arrow a φ t C).reach (Ty.arrow a' φ' t' C').reach →
      LeLive S (.arrow a φ t C) (.arrow a' φ' t' C')
  | eff : (Stage.use (.rt r e) C).le (Stage.use (.rt r' e') C') →
      LeLive S t t' →
      Caps.Sub (Ty.eff r e t C).reach (Ty.eff r' e' t' C').reach →
      LeLive S (.eff r e t C) (.eff r' e' t' C')
  /-- A record is immutable, so it compares field by field, covariantly; it keeps its labels
  and what it reaches, as an arrow does. -/
  | rcons : LeLive S t t' → LeLive S r r' →
      Caps.Sub (Ty.rcons l t r).reach (Ty.rcons l t' r').reach →
      LeLive S (.rcons l t r) (.rcons l t' r')
  /-- Data with hidden storage may widen its hidden row and nothing else (regions.md §2.2):
  `Sig.hidden_ok` keeps the row covariant in the fields. -/
  | hide {T : Name} {i : Nat} {us us' : List Ty} {a b : Ty} : S.hidden T = some i →
      us.length = us'.length → (∀ j, j ≠ i → us[j]? = us'[j]?) →
      (∀ r r', us[i]? = some r → us'[i]? = some r' → Caps.Sub r.reach r'.reach) →
      a = Ty.tcon T us → b = Ty.tcon T us' → LeLive S a b
  /-- Data compares its arguments by the variance of each parameter (regions.md §2.2.1),
  its hidden row by what it reaches (as `hide`), and each argument keeps what it reaches. -/
  | data {T : Name} {us us' : List Ty} {a b : Ty} : us.length = us'.length →
      (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' →
        Caps.Sub u.reach u'.reach) →
      (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
        S.var T j = .pos → LeLive S u u') →
      (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
        S.var T j = .neg → LeLive S u' u) →
      (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
        S.var T j = .inv → Ty.Equiv u u') →
      a = Ty.tcon T us → b = Ty.tcon T us' → LeLive S a b

/-- A type application or a type name. -/
def Ty.isData : Ty → Bool
  | .adt _ => true
  | .tapp _ _ => true
  | _ => false

theorem Ty.foldl_tapp_isData (us : List Ty) : ∀ acc : Ty, acc.isData = true →
    (us.foldl .tapp acc).isData = true := by
  induction us with
  | nil => intro acc h; exact h
  | cons u us ih => intro acc _; exact ih _ rfl

theorem Ty.tcon_isData (T : Name) (us : List Ty) : (Ty.tcon T us).isData = true :=
  Ty.foldl_tapp_isData us _ rfl

/-- A type that subtyping relates only to equivalent types: not an arrow,
a computation, a record field or data. -/
def Ty.isPlain : Ty → Bool
  | .arrow .. | .eff .. | .rcons .. | .adt _ | .tapp _ _ => false
  | _ => true

theorem Ty.ne_tcon_of_plain {t : Ty} (h : t.isPlain = true) {T : Name} {us : List Ty} :
    t ≠ Ty.tcon T us := by
  intro e
  have := Ty.tcon_isData T us
  rw [← e] at this
  cases t <;> simp_all [Ty.isPlain, Ty.isData]

theorem Ty.arrow_ne_tcon {a φ b C T us} : Ty.arrow a φ b C ≠ Ty.tcon T us := by
  intro h; have := Ty.tcon_isData T us; rw [← h] at this; simp [Ty.isData] at this

theorem Ty.eff_ne_tcon {r e t C T us} : Ty.eff r e t C ≠ Ty.tcon T us := by
  intro h; have := Ty.tcon_isData T us; rw [← h] at this; simp [Ty.isData] at this

theorem Ty.rcons_ne_tcon {l t r T us} : Ty.rcons l t r ≠ Ty.tcon T us := by
  intro h; have := Ty.tcon_isData T us; rw [← h] at this; simp [Ty.isData] at this

/-- Polymorphic instantiation uses live subtyping at its leaves. -/
inductive LeLiveI (S : Sig) : Ty → Ty → Prop
  | le : LeLive S a b → LeLiveI S a b
  | inst {us : List Ty} : us.length = k → LeLiveI S (t.substT (instT us)) T → LeLiveI S (.all k t) T

theorem LeLiveI.refl : LeLiveI S t t := .le .refl

/-- Subtyping cannot erase the stage needed to call a qualified function. -/
theorem LeLive.arrow_use {a ψ t C a' ψ' t' C'}
    (h : LeLive S (.arrow a ψ t C) (.arrow a' ψ' t' C')) :
    (Stage.use ψ C).le (Stage.use ψ' C') := by
  cases h with
  | refl =>
    cases ψ with
    | pure => trivial
    | rt _ _ => exact ⟨fun _ h => h, fun _ h => h⟩
  | arrow _ hψ _ _ => exact hψ
  | equiv he =>
    cases he.form with
    | rows hr _ => cases hr
    | arrow =>
      cases ψ with
      | pure => trivial
      | rt _ _ => exact ⟨fun _ h => h, fun _ h => h⟩
  | hide _ _ _ _ ha => exact absurd ha Ty.arrow_ne_tcon
  | data _ _ _ _ _ ha => exact absurd ha Ty.arrow_ne_tcon

/-- The same obligation is kept when a qualified task is widened. -/
theorem LeLive.eff_use {r ε t C r' ε' t' C'}
    (h : LeLive S (.eff r ε t C) (.eff r' ε' t' C')) :
    (Stage.use (.rt r ε) C).le (Stage.use (.rt r' ε') C') := by
  cases h with
  | refl => exact ⟨fun _ h => h, fun _ h => h⟩
  | eff hψ _ _ => exact hψ
  | equiv he =>
    cases he.form with
    | rows hr _ => cases hr
    | eff => exact ⟨fun _ h => h, fun _ h => h⟩
  | hide _ _ _ _ ha => exact absurd ha Ty.eff_ne_tcon
  | data _ _ _ _ _ ha => exact absurd ha Ty.eff_ne_tcon

end Flick
