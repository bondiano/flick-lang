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

The schemes of `methSch` and `constSch` are written by hand (`bindTy`, `map2Ty`; `each_P`,
`pair` and `unc n` have no Prelude name, `each_R` is the generated `Seq.each_loop`); the fields of `S` equate the first with the ones generated from the checker (`PreludeSchemes`) by
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
  | 0 => some (cEq, ⟨1, [⟨cEq, [.var 0]⟩], arrP (.var 0) (arrP (.var 0) boolTy), .cnil, []⟩)
  | 1 => some (cApp, ⟨3, [⟨cApp, [.var 0]⟩], arrP (.var 2) (appQ (.var 0) (.var 1) (.var 2)), .cnil,
      [(.var 2, .var 1)]⟩)
  | 2 => some (cApp, ⟨7, [⟨cApp, [.var 0]⟩],
      map2Ty (.var 0) (.var 1) (.var 2) (.var 3) (.var 4) (.var 5) (.var 6), .cnil, []⟩)
  | 3 => some (cMonad, ⟨6, [⟨cMonad, [.var 0]⟩],
      bindTy (.var 0) (.var 1) (.var 2) (.var 3) (.var 4) (.var 5), .cnil, []⟩)
  | 4 => some (cAlt, ⟨3, [⟨cAlt, [.var 0]⟩], appQ (.var 0) (.var 2) (.var 1), .cnil, []⟩)
  | 5 => some (cIter, ⟨3, [⟨cIter, [.var 0]⟩],
      arrP (appQ (.var 0) (.var 1) (.var 2)) (appQ (.con 9) (.var 1) (.var 2)),
      .cnil, []⟩)
  | _ => none

/-- The constants of Γ₀: `pair`, `None`, `Seq.flat_map`, `singleton`, `empty`, `to_list`,
`each_P`, `each_R`, `()`, then `unc n` at `2n + 10` and `from_result` at `11`. -/
def constSch : Nat → Option Scheme
  | 0 => some ⟨2, [], arrP (.var 0) (arrP (.var 1) (.app (.app (.con 10) (.var 0)) (.var 1))), .cnil, []⟩
  | 1 => some ⟨1, [], optTy (.var 0), .cnil, []⟩
  | 2 => some ⟨5, [], bindTy (.con 9) (.var 2) (.var 3) (.var 0) (.var 1) (.var 4), .cnil, []⟩
  | 3 => some ⟨2, [], arrP (.var 0) (appQ (.con 9) (.var 1) (.var 0)), .cnil, []⟩
  | 4 => some ⟨2, [], appQ (.con 9) (.var 1) (.var 0), .cnil, []⟩
  | 5 => some ⟨2, [], arrP (appQ (.con 9) (.var 1) (.var 0)) (.app (.con 8) (.var 0)), .cnil, []⟩
  | 6 => some ⟨3, [], arrP (.arr .P (.var 2) (.con 1) (.var 0))
      (.arr .P (appQ (.con 9) (.var 1) (.var 2)) (.con 1) (.var 0)), .cnil, []⟩
  | 7 => some (PreludeSchemes.Seq.each_loop 1 9)
  | 8 => some ⟨0, [], .con 1, .cnil, []⟩
  | 9 => none
  | n + 10 => if n % 2 = 0 then (if 0 < n / 2 then some (uncSch 10 (n / 2)) else none)
      else if n = 1 then some (PreludeSchemes.from_result 11 12) else none

/-- The constants of the witness have no value bounds. -/
theorem constSch_bd {c : Nat} {s : Scheme} (h : constSch c = some s) : s.bd = [] := by
  unfold constSch at h
  repeat' split at h
  all_goals first | (cases h; rfl) | cases h

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

/-- The program: `Option` and `List` are covariant (`+`), `Eq` is contravariant (`-`),
and higher-kinded container classes remain invariant (`inv`). -/
def P : Prog where
  supers := supers
  insts := insts
  structC C := C == cEq
  fields τ := if τ = tyRec then some [tyInt] else none
  primC _ := false
  prims _ := none
  methods := methSch
  consts := constSch
  ctors _ := none
  dataVar T j := if (T = 3 ∨ T = 8) ∧ j = 0 then .pos else .inv
  clsVar C j := if C = cEq ∧ j = 0 then .neg else .inv

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
  fromResult := 11
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
  hFromResult := rfl

/-- The size of a type; a constructor counts by its number, so `Int < Rec`. -/
def tsize : Ty → Nat
  | .var _ => 1
  | .con c => c + 1
  | .app t u => tsize t + tsize u + 1
  | .arr _ a b q => tsize a + tsize b + tsize q + 1
  | .cnil => 1
  | .cun q q' | .run q q' => tsize q + tsize q' + 1
  | .rnm _ t | .all _ t => tsize t + 1

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

/-- Computable class-variance inference derives `Eq` contravariant (`-`) on the witness. -/
theorem eq_clsVar_inferred :
    inferClassVar P.dataVar P.clsVar 0 (P.supers cEq)
      [([], arrP (.var 0) (arrP (.var 0) boolTy))] = .neg := by
  simp [inferClassVar, inferMethodVar, P, supers, cEq, arrP, boolTy,
    inferTyVar_arr, inferTyVar_var, inferTyVar_of_false, Ty.occursB, Stage.occursB, Variance.join]

private theorem foldl_app_isApp : ∀ (xs : List Ty) (acc : Ty),
    (∃ f u, acc = .app f u) → ∃ f u, xs.foldl .app acc = .app f u
  | [], _, h => h
  | x :: xs, acc, _ => foldl_app_isApp xs (.app acc x) ⟨acc, x, rfl⟩

private theorem tcon_eq_con {T c : Nat} {us : List Ty} (h : Ty.tcon T us = .con c) :
    T = c ∧ us = [] := by
  cases us with
  | nil => cases h; exact ⟨rfl, rfl⟩
  | cons x xs =>
    obtain ⟨f, u, he⟩ := foldl_app_isApp xs (.app (.con T) x) ⟨.con T, x, rfl⟩
    rw [Ty.tcon] at h; simp only [List.foldl_cons] at h; rw [he] at h; cases h

private theorem tyLe_con_left {c : Nat} {b : Ty} (h : Ty.Le P.dataVar (.con c) b) : b = .con c := by
  generalize ha : Ty.con c = a at h
  cases h with
  | refl => rfl
  | arr => cases ha
  | data _ _ hl _ _ _ _ =>
    obtain ⟨rfl, rfl⟩ := tcon_eq_con ha.symm
    cases ‹List Ty› with
    | nil => rfl
    | cons _ _ => simp at hl

private theorem tyLe_con_right {c : Nat} {a : Ty} (h : Ty.Le P.dataVar a (.con c)) : a = .con c := by
  generalize hb : Ty.con c = b at h
  cases h with
  | refl => rfl
  | arr => cases hb
  | data _ _ hl _ _ _ _ =>
    obtain ⟨rfl, rfl⟩ := tcon_eq_con hb.symm
    cases ‹List Ty› with
    | nil => rfl
    | cons _ _ => simp at hl

private theorem serves_con {C c : Nat} {Q : Cn} (h : Cn.Serves P Q ⟨C, [.con c]⟩) :
    Q = ⟨C, [.con c]⟩ := by
  cases Q with
  | mk C' args =>
    obtain ⟨hc, hl, hpos, hneg, hinv⟩ := h
    simp only at hc hl
    subst hc
    cases args with
    | nil => simp at hl
    | cons u rest =>
      cases rest with
      | cons _ _ => simp at hl
      | nil =>
        congr 1; congr 1
        cases hv : P.clsVar C 0 with
        | pos => exact tyLe_con_left (hpos 0 u (.con c) rfl rfl hv)
        | neg => exact tyLe_con_right (hneg 0 u (.con c) rfl rfl hv).1
        | inv => exact hinv 0 u (.con c) rfl rfl (.inl hv)
        | bi => exact hinv 0 u (.con c) rfl rfl (.inr hv)

theorem coherent : Coherent P lt where
  k1' {I J ins jns Q σ σ'} hI hJ hsI hsJ := by
    simp only [P] at hI hJ
    unfold insts at hI hJ
    split at hI <;> cases hI <;> split at hJ <;> cases hJ
    all_goals first
      | rfl
      | (simp only [List.map_cons, List.map_nil, tyInt, Ty.subst] at hsI hsJ;
         have h1 := serves_con hsI; have h2 := serves_con hsJ;
         rw [h1] at h2; simp_all [cEq, cApp, cMonad, cAlt, cIter])
  super_serves {Q H i S_Q S_H} hserv hQ hH := by
    have hcls : H.cls = Q.cls := hserv.1
    cases h : Q.cls with
    | zero => rw [Prog.super, h] at hQ; simp [P, supers] at hQ
    | succ n =>
      have hinv : ∀ j, P.clsVar Q.cls j = .inv ∨ P.clsVar Q.cls j = .bi := fun j =>
        .inl (by simp [P, cEq, h])
      obtain rfl := Cn.Serves.eq_of_inv hinv hserv
      rw [hQ] at hH; cases hH; exact .refl _
  lt_wf := InvImage.wf measure Nat.lt_wfRel.wf
  lt_trans := Nat.lt_trans
  k5inst hI hc _ := by rw [insts_some hI] at hc; cases hc
  k5mono hI _ _ _ hc := by rw [insts_some hI] at hc; cases hc
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
  k6struct {C τ ts I ins Q σ} hC hτ hI hsS hsI := by
    simp only [P] at hτ hI hC; split at hτ <;> cases hτ; subst_vars
    have h1 := serves_con hsS
    unfold insts at hI; split at hI <;> cases hI
    all_goals
      simp only [List.map_cons, List.map_nil, tyInt, Ty.subst] at hsI
      have h2 := serves_con hsI; rw [h1] at h2
      simp_all [cEq, cApp, cMonad, cAlt, cIter, tyRec]
  primStruct h := by cases h
  primSupers h := by cases h
  primVar h := by cases h
  structSupers {C i Sup} hC h := by
    simp only [P] at hC h; simp at hC; subst hC; simp [supers, cEq] at h
  fields_serves {C τ₀ τ₁ τ₂ ts₁ ts₂} hC h1 h2 hs₁ hs₂ := by
    simp only [P] at hC h1 h2
    simp at hC; subst hC
    split at h1 <;> cases h1
    split at h2 <;> cases h2
    subst_vars
    have h0 := serves_con hs₁
    simp only [Cn.mk.injEq, List.cons.injEq, and_true, true_and] at h0
    subst h0
    refine ⟨rfl, fun j u v hu hv => ?_⟩
    cases j with
    | zero =>
      simp at hu hv; subst hu; subst hv
      exact ⟨⟨cEq, [tyInt]⟩, by simp [lt, measure, tyInt, tsize], .refl _, .refl _⟩
    | succ j => simp at hu
  fields_sub {C τ_Q τ_H ts_H} hC hH hs := by
    simp only [P] at hC hH
    simp at hC; subst hC
    split at hH <;> cases hH
    subst_vars
    have h0 := serves_con hs
    simp only [Cn.mk.injEq, List.cons.injEq, and_true, true_and] at h0
    subst h0
    exact ⟨[tyInt], by simp [P, tyRec], rfl, fun j u v hu hv => by
      cases j with
      | zero => simp at hu hv; subst hu; subst hv; exact .refl _
      | succ j => simp at hu⟩
  k7 {I ins i Sup} hI h := by
    simp only [P] at hI h
    unfold insts at hI
    split at hI <;> cases hI <;>
      (simp only [Prog.super] at h; unfold supers at h) <;>
      (try simp [cEq, cApp, cMonad, cAlt, cIter] at h) <;>
      (rcases i with _ | i <;> simp at h; subst h) <;>
      first
        | exact ⟨_, rfl, .inst_eq (I := 1) (σ := Ty.var) (ds := []) rfl rfl rfl .nil⟩
        | exact ⟨_, rfl, .inst_eq (I := 3) (σ := Ty.var) (ds := []) rfl rfl rfl .nil⟩
  fields_subst {τ ts θ} h := by
    simp only [P] at h ⊢; split at h <;> cases h; subst_vars; simp [tyRec, tyInt, Ty.subst]
  prims_subst h := by cases h

end Flick.Elab.Witness
