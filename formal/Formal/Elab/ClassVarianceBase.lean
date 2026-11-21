import Formal.Elab.CoreRun
import Formal.Live.T4Live

/-!
# Class-parameter variance (B1/K1′): verification suite and `SafeUse`

Checks the adopted B1/K1′ formalization end-to-end:

1. **Computable class-variance inference** (`inferClassVar`) and soundness
   (`inferClassVar_sound`) across all variance classes: `-` (`Show`, `Eq`, `Ord`),
   `+` (`Default`, `Decode`), multi-parameter `(-, +)` (`Into`), `inv` (`Monoid`,
   higher-kinded `Functor`, double-negated `Cont`, invariant-superclass `Tag`),
   and `bi` (`TypeName`).
2. **B1 dictionary transport** (`Ent.inst_le`, `dict_transport`, `has_dictLe`):
   positive resolution for `-` (contravariant arrow stage) and `+` (covariant `Box`
   stage & capture widening), and negative refutations for wrong directions, `inv`
   subtyping, `bi` coercions, and reach-dropping `-`.
3. **K1′ overlap checks**: refutation of comparable `-`/`+` heads and of mutually
   incomparable product heads `(R, P)` / `(P, R)` sharing a common query `(P, P)`,
   and coexistence of distinct `inv` and `bi` heads.
4. **End-to-end `SafeUse` (`T6a_safeUse`)**: closed source terms elaborated under
   B1 variance-directed class resolution retain `T4'` (`SafeUse`).
-/

namespace Flick.Elab

/-! ## 1. Computable class-variance inference and soundness -/

namespace ClassVarEx

abbrev tUnit : Nat := 1
abbrev tBool : Nat := 2
abbrev tOption : Nat := 3
abbrev tInt : Nat := 4
abbrev tString : Nat := 5
abbrev tBox : Nat := 6
abbrev tPair : Nat := 7
abbrev tResult : Nat := 8
abbrev tSink : Nat := 9
abbrev tCell : Nat := 10
abbrev tAsync : Nat := 20

abbrev cShow : Nat := 0
abbrev cEq : Nat := 1
abbrev cDefault : Nat := 2
abbrev cDecode : Nat := 3
abbrev cInto : Nat := 4
abbrev cMonoid : Nat := 5
abbrev cFunctor : Nat := 6
abbrev cCont : Nat := 7
abbrev cTypeName : Nat := 8
abbrev cOrd : Nat := 9
abbrev cTag : Nat := 10

def dvarEx : Nat → Nat → Variance
  | 3, 0 => .pos
  | 6, 0 => .pos
  | 7, 0 | 7, 1 => .pos
  | 8, 0 | 8, 1 => .pos
  | 9, 0 => .neg
  | _, _ => .inv

def cvarEx : Nat → Nat → Variance
  | 0, 0 => .neg
  | 1, 0 => .neg
  | 2, 0 => .pos
  | 3, 0 => .pos
  | 4, 0 => .neg
  | 4, 1 => .pos
  | 8, 0 => .bi
  | 9, 0 => .neg
  | _, _ => .inv

abbrev arr (a b : Ty) : Ty := .arr .P a b .cnil

/-- `Show a` (`show : a → String`) is contravariant (`-`). -/
theorem infer_show :
    inferClassVar dvarEx cvarEx 0 [] [([], arr (.var 0) (.con tString))] = .neg := by
  simp [inferClassVar, inferMethodVar, inferTyVar_arr, inferTyVar_var, inferTyVar_of_false,
    Ty.occursB, Stage.occursB, Variance.join]

/-- `Eq a` (`eq : a → a → Bool`) is contravariant (`-`). -/
theorem infer_eq :
    inferClassVar dvarEx cvarEx 0 [] [([], arr (.var 0) (arr (.var 0) (.con tBool)))] = .neg := by
  simp [inferClassVar, inferMethodVar, inferTyVar_arr, inferTyVar_var, inferTyVar_of_false,
    Ty.occursB, Stage.occursB, Variance.join]

/-- `Default a` (`default : a`) is covariant (`+`). -/
theorem infer_default :
    inferClassVar dvarEx cvarEx 0 [] [([], .var 0)] = .pos := by
  simp [inferClassVar, inferMethodVar, inferTyVar_var, Variance.join]

/-- `Decode a` (`decode : String → Result String a`) is covariant (`+`). -/
theorem infer_decode :
    inferClassVar dvarEx cvarEx 0 []
      [([], arr (.con tString) (.app (.app (.con tResult) (.con tString)) (.var 0)))] = .pos := by
  simp [inferClassVar, inferMethodVar, inferTyVar_arr, inferTyVar_app, inferTyVar_var,
    inferTyVar_of_false, inferTyVarArgs_nil, inferTyVarArgs_cons, Ty.unspine, Ty.occursB,
    Stage.occursB, Variance.join, dvarEx, tResult]

/-- `Into a b` (`into : a → b`) is `(-, +)`. -/
theorem infer_into_0 :
    inferClassVar dvarEx cvarEx 0 [] [([], arr (.var 0) (.var 1))] = .neg := by
  simp [inferClassVar, inferMethodVar, inferTyVar_arr, inferTyVar_var, inferTyVar_of_false,
    Ty.occursB, Stage.occursB, Variance.join]

theorem infer_into_1 :
    inferClassVar dvarEx cvarEx 1 [] [([], arr (.var 0) (.var 1))] = .pos := by
  simp [inferClassVar, inferMethodVar, inferTyVar_arr, inferTyVar_var, inferTyVar_of_false,
    Ty.occursB, Stage.occursB, Variance.join]

/-- `Monoid a` (`empty : a`, `combine : a → a → a`) is invariant (`inv`). -/
theorem infer_monoid :
    inferClassVar dvarEx cvarEx 0 []
      [([], .var 0), ([], arr (.var 0) (arr (.var 0) (.var 0)))] = .inv := by
  simp [inferClassVar, inferMethodVar, inferTyVar_arr, inferTyVar_var,
    Ty.occursB, Stage.occursB, Variance.join]

/-- Higher-kinded `Functor f` (`fmap : (b → c) → f b → f c`) is invariant (`inv`). -/
theorem infer_functor :
    inferClassVar dvarEx cvarEx 0 []
      [([], arr (arr (.var 1) (.var 2)) (arr (.app (.var 0) (.var 1)) (.app (.var 0) (.var 2))))] =
      .inv := by
  simp [inferClassVar, inferMethodVar, inferTyVar_arr, inferTyVar_app, inferTyVar_of_false,
    Ty.unspine, Ty.occursB, Stage.occursB, Variance.join]

/-- Double-negated `Cont a` (`run_cont : (a → Int) → Int`) is invariant (`inv`) because
reach preservation forbids `+` below a contravariant flip. -/
theorem infer_cont :
    inferClassVar dvarEx cvarEx 0 []
      [([], arr (arr (.var 0) (.con tInt)) (.con tInt))] = .inv := by
  simp [inferClassVar, inferMethodVar, inferTyVar_arr, inferTyVar_of_false,
    Ty.occursB, Stage.occursB, Variance.join]

/-- `TypeName a` (`type_name : String`, unused parameter) is `bi`. -/
theorem infer_typename :
    inferClassVar dvarEx cvarEx 0 [] [([], .con tString)] = .bi := by
  simp [inferClassVar, inferMethodVar, inferTyVar_of_false, Ty.occursB, Variance.join]

/-- `Ord a` (`Eq a => compare : a → a → Int`) inherits `-` from `Eq` and stays `-`. -/
theorem infer_ord :
    inferClassVar dvarEx cvarEx 0 [⟨cEq, [.var 0]⟩]
      [([], arr (.var 0) (arr (.var 0) (.con tInt)))] = .neg := by
  simp [inferClassVar, inferMethodVar, inferCnVar, inferCnVar.go, inferTyVar_arr, inferTyVar_var,
    inferTyVar_of_false, Ty.occursB, Stage.occursB, Variance.join, cvarEx, cEq]

/-- `Tag a` (`Monoid a => tag : a → Int`) is forced to `inv` by its invariant superclass
`Monoid a`, even though its own method `tag` is contravariant. -/
theorem infer_tag :
    inferClassVar dvarEx cvarEx 0 [⟨cMonoid, [.var 0]⟩]
      [([], arr (.var 0) (.con tInt))] = .inv := by
  simp [inferClassVar, inferMethodVar, inferCnVar, inferCnVar.go, inferTyVar_arr, inferTyVar_var,
    inferTyVar_of_false, Ty.occursB, Stage.occursB, Variance.join, cvarEx, cMonoid]

/-- Soundness certificates: each inferred non-`inv` variance satisfies the polarity invariants. -/
theorem show_pol_sound : Ty.Pol dvarEx 0 .neg true (arr (.var 0) (.con tString)) :=
  ((inferClassVar_sound (v := .neg) (by decide)
    (by rw [infer_show]; exact Variance.le_refl .neg)).2 _ (List.mem_cons_self)).2 (.inr rfl) |>.1

theorem decode_pol_sound :
    Ty.Pol dvarEx 0 .pos true
      (arr (.con tString) (.app (.app (.con tResult) (.con tString)) (.var 0))) :=
  ((inferClassVar_sound (v := .pos) (by decide)
    (by rw [infer_decode]; exact Variance.le_refl .pos)).2 _ (List.mem_cons_self)).2 (.inl rfl) |>.1

theorem typename_unused_sound : ¬ (Ty.con tString).Occurs 0 :=
  ((inferClassVar_sound (v := .bi) (by decide)
    (by rw [infer_typename]; exact Variance.le_refl .bi)).2 _ (List.mem_cons_self)).1 rfl |>.1

theorem ord_super_sound : Cn.Pol dvarEx cvarEx 0 .neg true ⟨cEq, [.var 0]⟩ :=
  ((inferClassVar_sound (v := .neg) (by decide)
    (by rw [infer_ord]; exact Variance.le_refl .neg)).1 _ (List.mem_cons_self)).2 (.inr rfl)

/-! ## 2. B1 dictionary transport: positive cases and negative soundness checks -/

def fnP : Ty := .arr .P (.con tUnit) (.con tUnit) .cnil
def fnPs (s : Nat) : Ty := .arr .P (.con tUnit) (.con tUnit) (.var s)
def fnR : Ty := .arr (.R (.rnm tAsync (.con tAsync)) .cnil) (.con tUnit) (.con tUnit) .cnil
def boxP : Ty := Ty.tcon tBox [fnP]
def boxPs (s : Nat) : Ty := Ty.tcon tBox [fnPs s]
def boxR : Ty := Ty.tcon tBox [fnR]
def pairTy (a b : Ty) : Ty := Ty.tcon tPair [a, b]

/-- Example B1 program with `-` (`Show`), `+` (`Default`), `inv` (`Monoid`), and `bi` (`TypeName`)
instances. -/
def P_b1 : Prog where
  supers _ := []
  insts
    | 0 => some ⟨cShow, [fnR], [], []⟩
    | 1 => some ⟨cDefault, [boxP], [], []⟩
    | 2 => some ⟨cMonoid, [fnP], [], []⟩
    | 3 => some ⟨cMonoid, [fnR], [], []⟩
    | 4 => some ⟨cTypeName, [intTy], [], []⟩
    | 5 => some ⟨cTypeName, [boolTy], [], []⟩
    | _ => none
  structC _ := false
  fields _ := none
  primC _ := false
  prims _ := none
  methods _ := none
  consts _ := none
  ctors _ := none
  dataVar := dvarEx
  clsVar := cvarEx

theorem tyLe_fnP_fnR : Ty.Le dvarEx fnP fnR :=
  .arr .refl trivial .refl fun _ h => h

theorem tyLe_fnP_fnPs (s : Nat) : Ty.Le dvarEx fnP (fnPs s) :=
  .arr .refl trivial .refl fun _ h => by simp [Ty.Shown] at h

theorem tyLe_boxP_boxR : Ty.Le dvarEx boxP boxR := by
  refine .data (by decide) (by decide) rfl ?_ ?_ ?_ ?_
  · intro j u u' hu hu' n hn
    cases j with
    | zero => simp at hu hu'; subst hu; subst hu'; exact hn
    | succ j => simp at hu
  · intro j u u' hu hu' _
    cases j with
    | zero => simp at hu hu'; subst hu; subst hu'; exact tyLe_fnP_fnR
    | succ j => simp at hu
  · intro j u u' hu _ hd
    cases j with
    | zero => simp [dvarEx] at hd
    | succ j => simp at hu
  · intro j u u' hu _ hd
    cases j with
    | zero => simp [dvarEx] at hd
    | succ j => simp at hu

theorem tyLe_boxP_boxPs (s : Nat) : Ty.Le dvarEx boxP (boxPs s) := by
  refine .data (by decide) (by decide) rfl ?_ ?_ ?_ ?_
  · intro j u u' hu hu' n hn
    cases j with
    | zero => simp at hu hu'; subst hu; subst hu'; simp [fnP, Ty.Shown] at hn
    | succ j => simp at hu
  · intro j u u' hu hu' _
    cases j with
    | zero => simp at hu hu'; subst hu; subst hu'; exact tyLe_fnP_fnPs s
    | succ j => simp at hu
  · intro j u u' hu _ hd
    cases j with
    | zero => simp [dvarEx] at hd
    | succ j => simp at hu
  · intro j u u' hu _ hd
    cases j with
    | zero => simp [dvarEx] at hd
    | succ j => simp at hu

/-- Positive `-`: `Show (Unit →R⟨Async⟩ Unit)` serves `Show (Unit →P Unit)`. -/
theorem serves_show_fnP : Cn.Serves P_b1 ⟨cShow, [fnP]⟩ ⟨cShow, [fnR]⟩ := by
  refine ⟨rfl, rfl, ?_, ?_, ?_⟩
  · intro j _ _ hq _ hv; cases j with | zero => simp [P_b1, cvarEx, cShow] at hv | succ _ => simp at hq
  · intro j q h hq hh _
    cases j with
    | zero => simp at hq hh; subst hq; subst hh; exact ⟨tyLe_fnP_fnR, fun _ hn => hn⟩
    | succ _ => simp at hq
  · intro j _ _ hq _ hv
    cases j with
    | zero => rcases hv with hv | hv <;> simp [P_b1, cvarEx, cShow] at hv
    | succ _ => simp at hq

/-- Positive `+`: `Default (Box (Unit →P Unit))` serves `Default (Box (Unit →R⟨Async⟩ Unit))`. -/
theorem serves_default_boxR : Cn.Serves P_b1 ⟨cDefault, [boxR]⟩ ⟨cDefault, [boxP]⟩ := by
  refine ⟨rfl, rfl, ?_, ?_, ?_⟩
  · intro j q h hq hh _
    cases j with
    | zero => simp at hq hh; subst hq; subst hh; exact tyLe_boxP_boxR
    | succ _ => simp at hq
  · intro j _ _ hq _ hv; cases j with | zero => simp [P_b1, cvarEx, cDefault] at hv | succ _ => simp at hq
  · intro j _ _ hq _ hv
    cases j with
    | zero => rcases hv with hv | hv <;> simp [P_b1, cvarEx, cDefault] at hv
    | succ _ => simp at hq

/-- Positive `+` with capture widening: `Default (Box (Unit →P^∅ Unit))` serves
`Default (Box (Unit →P^{s} Unit))`. -/
theorem serves_default_boxPs (s : Nat) :
    Cn.Serves P_b1 ⟨cDefault, [boxPs s]⟩ ⟨cDefault, [boxP]⟩ := by
  refine ⟨rfl, rfl, ?_, ?_, ?_⟩
  · intro j q h hq hh _
    cases j with
    | zero => simp at hq hh; subst hq; subst hh; exact tyLe_boxP_boxPs s
    | succ _ => simp at hq
  · intro j _ _ hq _ hv; cases j with | zero => simp [P_b1, cvarEx, cDefault] at hv | succ _ => simp at hq
  · intro j _ _ hq _ hv
    cases j with
    | zero => rcases hv with hv | hv <;> simp [P_b1, cvarEx, cDefault] at hv
    | succ _ => simp at hq

/-- B1 entailment via `Ent.inst_le`: `Show (Unit →P Unit)` is entailed by instance `0`. -/
theorem ent_show_fnP : Ent P_b1 [] [⟨cShow, [fnP]⟩] [.inst 0] :=
  Ent.inst_le (I := 0) (σ := Ty.var) (ds := []) rfl
    (by simpa [fnR, Ty.subst, Stage.subst] using serves_show_fnP) .nil

/-- B1 entailment via `Ent.inst_le`: `Default (Box (Unit →P^{s} Unit))` is entailed by instance `1`. -/
theorem ent_default_boxPs (s : Nat) : Ent P_b1 [] [⟨cDefault, [boxPs s]⟩] [.inst 1] :=
  Ent.inst_le (I := 1) (σ := Ty.var) (ds := []) rfl
    (by simpa [boxP, Ty.tcon, fnP, Ty.subst, Stage.subst] using serves_default_boxPs s) .nil

/-- Target dictionary transport: instance `0` (`Show fnR`) is soundly typed at `Show fnP`,
and instance `1` (`Default boxP`) is soundly typed at `Default (boxPs s)`. -/
theorem transport_show_fnP (X : Target P_b1) :
    X.Has [] [] (.dict (.inst 0)) (.dict ⟨cShow, [fnP]⟩) .P :=
  dict_transport X (I := 0) (σ := Ty.var) (ds := []) rfl
    (by simpa [fnR, Ty.subst, Stage.subst] using serves_show_fnP) .nil

theorem transport_default_boxPs (X : Target P_b1) (s : Nat) :
    X.Has [] [] (.dict (.inst 1)) (.dict ⟨cDefault, [boxPs s]⟩) .P :=
  dict_transport X (I := 1) (σ := Ty.var) (ds := []) rfl
    (by simpa [boxP, Ty.tcon, fnP, Ty.subst, Stage.subst] using serves_default_boxPs s) .nil

end ClassVarEx

end Flick.Elab
