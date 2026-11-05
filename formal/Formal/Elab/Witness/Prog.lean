import Formal.Elab.Sugar

/-!
# Witness: a program that meets the hypotheses (the program and K1–K8)

A concrete program of `Elab`: the classes `Eq`, `Applicative`, `Monad`, `Alternative` and
`Iterable` with the methods of `Sugar`, and the instances

- `Eq Int` (`I0`), and `Eq` structural on the record `Rec = {x : Int}` (`struct_Eq`);
- `Applicative`/`Monad` of `Eff` (`I1`, `I2`), lazy (`docs/formal/elaboration.md#value-qualifiers-in-prelude-schemes`);
- `Applicative`/`Monad`/`Alternative` of `OptQ` (`I3`, `I4`, `I5`), an eager `Option`;
- `Iterable ListQ` (`I6`).

Classes over containers are over `m : CRow → Type → Type` (`docs/formal/elaboration.md#value-qualifiers-in-prelude-schemes`), so the `Option` and
`List` of the instances are `OptQ q a` and `ListQ q a` with a phantom `CRow` parameter. The
source's own `Option` (`optTy`, constructor `3`) stays as it is. `Functor` is left out: no
sugar uses it.

The type constructors are `unit = 1`, `Bool = 2`, `Option = 3`, `Int = 4`, `Eff = 5`,
`OptQ = 6`, `ListQ = 7`, `List = 8`, `Seq = 9`, the pair `Tup = 10`, `Result = 11`,
`Errors = 12`, `Rec = 13`.

The schemes of `methSch` and `constSch` are written by hand (`bindTy`, `map2Ty`, `eachSch`);
the fields of `S` equate them with the ones generated from the checker (`PreludeSchemes`) by
`rfl`, so a hand scheme that drifts from `Prelude.fl` breaks the build.
-/

namespace Flick.Elab.Witness

open Flick.Elab

def cEq : Nat := 0
def cApp : Nat := 1
def cMonad : Nat := 2
def cAlt : Nat := 3
def cIter : Nat := 4

/-- The Elab types the program uses. -/
def tyInt : Ty := .con 4
def tyRec : Ty := .con 13

/-- The schemes of the methods: `eq`, `pure`, `map2`, `flat_map`, `nothing`, `iter`. -/
def methSch : Nat → Option (Nat × Scheme)
  | 0 => some (cEq, ⟨1, [⟨cEq, [.var 0]⟩], arrP (.var 0) (arrP (.var 0) boolTy), .cnil⟩)
  | 1 => some (cApp, ⟨3, [⟨cApp, [.var 0]⟩], arrP (.var 2) (appQ (.var 0) (.var 1) (.var 2)), .cnil⟩)
  | 2 => some (cApp, ⟨8, [⟨cApp, [.var 0]⟩],
      map2Ty (.var 0) (.var 1) (.var 2) (.var 3) (.var 4) (.var 5) (.var 6) (.var 7), .cnil⟩)
  | 3 => some (cMonad, ⟨6, [⟨cMonad, [.var 0]⟩],
      bindTy (.var 0) (.var 1) (.var 2) (.var 3) (.var 4) (.var 5), .cnil⟩)
  | 4 => some (cAlt, ⟨3, [⟨cAlt, [.var 0]⟩], appQ (.var 0) (.var 2) (.var 1), .cnil⟩)
  | 5 => some (cIter, ⟨3, [⟨cIter, [.var 0]⟩],
      arrP (appQ (.var 0) (.var 1) (.var 2)) (appQ (.con 9) (.cun (.var 1) (.var 2)) (.var 2)),
      .cnil⟩)
  | _ => none

/-- The constants of Γ₀: `pair`, `None`, `Seq.flat_map`, `singleton`, `empty`, `to_list`,
`each_P`, `each_R`, `()`, then `unc n` at `2n + 10` and `from_result_n` at `2n + 11`. -/
def constSch : Nat → Option Scheme
  | 0 => some ⟨2, [], arrP (.var 0) (arrP (.var 1) (.app (.app (.con 10) (.var 0)) (.var 1))), .cnil⟩
  | 1 => some ⟨1, [], optTy (.var 0), .cnil⟩
  | 2 => some ⟨5, [], bindTy (.con 9) (.var 0) (.var 1) (.var 2) (.var 3) (.var 4), .cnil⟩
  | 3 => some ⟨2, [], arrP (.var 0) (appQ (.con 9) (.var 1) (.var 0)), .cnil⟩
  | 4 => some ⟨2, [], appQ (.con 9) (.var 1) (.var 0), .cnil⟩
  | 5 => some ⟨2, [], arrP (appQ (.con 9) (.var 0) (.var 1)) (.app (.con 8) (.var 1)), .cnil⟩
  | 6 => some ⟨3, [], arrP (.arr .P (.var 2) (.con 1) (.var 0))
      (.arr .P (appQ (.con 9) (.var 1) (.var 2)) (.con 1) (.var 0)), .cnil⟩
  | 7 => some ⟨5, [], arrP (.arr (.R (.var 3) (.var 4)) (.var 2) (.con 1) (.var 0))
      (.arr (.R (.var 3) (.var 4)) (appQ (.con 9) (.var 1) (.var 2)) (.con 1) (.var 0)), .cnil⟩
  | 8 => some ⟨0, [], .con 1, .cnil⟩
  | 9 => none
  | n + 10 => if n % 2 = 0 then (if 0 < n / 2 then some (uncSch 10 (n / 2)) else none)
      else some (frSch 11 12 (n / 2))

/-- The instances `I0 … I6`. -/
def insts : Nat → Option Inst
  | 0 => some ⟨cEq, [tyInt], [], []⟩
  | 1 => some ⟨cApp, [.con 5], [], []⟩
  | 2 => some ⟨cMonad, [.con 5], [], [.inst 1]⟩
  | 3 => some ⟨cApp, [.con 6], [], []⟩
  | 4 => some ⟨cMonad, [.con 6], [], [.inst 3]⟩
  | 5 => some ⟨cAlt, [.con 6], [], [.inst 3]⟩
  | 6 => some ⟨cIter, [.con 7], [], []⟩
  | _ => none

/-- `Monad` and `Alternative` have the superclass `Applicative`. -/
def supers : Nat → List Cn
  | 2 | 3 => [⟨cApp, [.var 0]⟩]
  | _ => []

/-- The program. -/
def P : Prog where
  supers := supers
  insts := insts
  structC C := C == cEq
  fields τ := if τ = tyRec then some [tyInt] else none
  primC _ := false
  prims _ := none
  methods := methSch
  consts := constSch

/-- The names of `Sugar`. -/
def S : Sugar P where
  tup := 10
  seq := 9
  list := 8
  unit := 1
  result := 11
  errors := 12
  monad := cMonad
  applicative := cApp
  alternative := cAlt
  iterable := cIter
  flatMap := 3
  map2 := 2
  pure := 1
  nothing := 4
  iter := 5
  pair := 0
  unc n := 2 * n + 10
  noneC := 1
  seqFlatMap := 2
  singleton := 3
  empty := 4
  toList := 5
  each φ := match φ with | .P => 6 | .R _ _ => 7
  unitV := 8
  fromResult n := 2 * n + 11
  hFlatMap := rfl
  hMap2 := rfl
  hPure := rfl
  hNothing := rfl
  hIter := rfl
  hPair := rfl
  hUnc n hn := by show constSch (2 * n + 10) = _; simp [constSch, hn]
  hNone := rfl
  hSeqFlatMap := rfl
  hSingleton := rfl
  hEmpty := rfl
  hToList := rfl
  hEach φ := by cases φ <;> rfl
  hUnitV := rfl
  hFromResult n := by
    show constSch (2 * n + 1 + 10) = _; simp only [constSch]; rw [if_neg (by omega)]; congr 3; omega

/-- The size of a type; a constructor counts by its number, so `Int < Rec`. -/
def tsize : Ty → Nat
  | .var _ => 1
  | .con c => c + 1
  | .app t u => tsize t + tsize u + 1
  | .arr _ a b q => tsize a + tsize b + tsize q + 1
  | .cnil => 1
  | .cun q q' => tsize q + tsize q' + 1

/-- The order of K5: `Monad` and `Alternative` above `Applicative`, then the size. -/
def measure (q : Cn) : Nat :=
  (if q.cls = cMonad ∨ q.cls = cAlt then 10 else 0) + (q.args.map tsize).sum

def lt (a b : Cn) : Prop := measure a < measure b

theorem size_getD_le (l : List Ty) : tsize (l[0]?.getD (.con 0)) ≤ max 1 (l.map tsize).sum := by
  cases l with
  | nil => simp [tsize]
  | cons t l => simp; omega

theorem insts_some {I ins} (h : insts I = some ins) : ins.ctx = [] := by
  unfold insts at h; split at h <;> cases h <;> rfl

theorem coherent : Coherent P lt where
  k1 {I J ins jns σ σ'} hI hJ hc hh := by
    simp only [P] at hI hJ
    unfold insts at hI hJ
    split at hI <;> cases hI <;> split at hJ <;> cases hJ <;>
      simp_all [cEq, cApp, cMonad, cAlt, cIter, tyInt, Ty.subst]
  k5vars hI hc := by rw [insts_some hI] at hc; cases hc
  lt_wf := InvImage.wf measure Nat.lt_wfRel.wf
  lt_trans := Nat.lt_trans
  k5inst hI hc := by rw [insts_some hI] at hc; cases hc
  k5super {Q i Sup} h := by
    simp only [Prog.super, P] at h
    unfold supers at h
    split at h <;> (rcases i with _ | i <;> simp at h) <;> subst h <;>
      simp_all [lt, measure, Cn.subst, Ty.subst, params, cApp, cMonad, cAlt] <;>
      have := size_getD_le Q.args <;> omega
  k5fields {C τ ts t} hC hτ ht := by
    simp only [P] at hτ; split at hτ <;> cases hτ
    simp at ht; subst ht; subst_vars; simp [lt, measure, tyInt, tyRec, tsize]
  k6prim h := by cases h
  k6struct {C τ ts I ins σ} hC hτ hI hc := by
    simp only [P] at hτ hI hC; split at hτ <;> cases hτ; subst_vars
    unfold insts at hI; split at hI <;> cases hI <;> simp_all [cEq, cApp, cMonad, cAlt, cIter,
      tyInt, tyRec, Ty.subst]
  primStruct h := by cases h
  primSupers h := by cases h
  structSupers {C i Sup} hC h := by
    simp only [P] at hC h; simp at hC; subst hC; simp [supers, cEq] at h
  k7 {I ins i Sup} hI h := by
    simp only [P] at hI h
    unfold insts at hI
    split at hI <;> cases hI <;>
      (simp only [Prog.super] at h; unfold supers at h) <;>
      (try simp [cEq, cApp, cMonad, cAlt, cIter] at h) <;>
      (rcases i with _ | i <;> simp at h; subst h) <;>
      first
        | exact ⟨_, rfl, .inst (I := 1) (σ := Ty.var) (ds := []) rfl rfl rfl .nil⟩
        | exact ⟨_, rfl, .inst (I := 3) (σ := Ty.var) (ds := []) rfl rfl rfl .nil⟩
  fields_subst {τ ts θ} h := by
    simp only [P] at h ⊢; split at h <;> cases h; subst_vars; simp [tyRec, tyInt, Ty.subst]
  prims_subst h := by cases h

end Flick.Elab.Witness
