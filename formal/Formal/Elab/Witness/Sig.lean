import Formal.Elab.Witness.Prog
import Formal.Elab.Witness.Closed
import Formal.Elab.LazyEff

/-!
# Witness: the names and the signature of the core, `Real`

`N` names the core's types, records, labels and globals; `Sg` declares the constructors
(`MkEff : eff ∅ ∅ b {q} → Eff q b`, the lists `Seq q a` and `ListQ q a` and the option
`OptQ q a` with a phantom `q`, `Result e a` with `Ok` and `Err`, …), one record type `D_C` per class and number of parameters
(`N.cls C n = 8n + C`, classes from `5` on share the empty record `7`), and a global per
instance, constant and structural dictionary. `real` proves `Real P Sg N`.
-/

namespace Flick.Elab.Witness

open Flick.Elab

def conName : Nat → Name
  | 1 => "Unit" | 3 => "Option" | 4 => "Int" | 5 => "Eff" | 6 => "OptQ" | 7 => "ListQ"
  | 8 => "List" | 9 => "Seq" | 10 => "Tup" | 11 => "Result" | 12 => "Errors" | 13 => "Rec"
  | _ => "Other"

def methName : Nat → Label
  | 0 => "eq" | 1 => "pure" | 2 => "map2" | 3 => "flat_map" | 4 => "nothing" | 5 => "iter"
  | _ => "meth"

def N : Names where
  con := conName
  cls C n := n * 8 + (if C < 5 then C else 7)
  meth := methName
  slot _ := "slot"
  inst I := 4 * I
  const c := 4 * c + 1
  strct _ _ := 2
  prim _ _ := 3
  some := "Some"

def listT (t : Flick.Ty) : Flick.Ty := .tapp (.adt "List") t

/-- `Seq q a` and `ListQ q a` of the core: lists with a phantom `CRow` parameter `q`. -/
def seqT (q a : Flick.Ty) : Flick.Ty := .tapp (.tapp (.adt "Seq") q) a
def lqT (q a : Flick.Ty) : Flick.Ty := .tapp (.tapp (.adt "ListQ") q) a

def ctorF : Name → Option (Nat × List Flick.Ty × Name)
  | "Some" => some (1, [.tvar 0], "Option")
  | "None" => some (1, [], "Option")
  | "MkEff" => some (2, [effField], "Eff")
  | "SomeQ" => some (2, [.tvar 1], "OptQ")
  | "NoneQ" => some (2, [], "OptQ")
  | "NilQ" => some (2, [], "ListQ")
  | "ConsQ" => some (2, [.tvar 1, lqT (.tvar 0) (.tvar 1)], "ListQ")
  | "Nil" => some (1, [], "List")
  | "Cons" => some (1, [.tvar 0, listT (.tvar 0)], "List")
  | "SNil" => some (2, [], "Seq")
  | "SCons" => some (2, [.tvar 1, seqT (.tvar 0) (.tvar 1)], "Seq")
  | "Tup" => some (2, [.tvar 0, .tvar 1], "Tup")
  | "MkUnit" => some (0, [], "Unit")
  | "MkRec" => some (0, [.base .int], "Rec")
  | "Ok" => some (2, [.tvar 1], "Result")
  | "Err" => some (2, [.tvar 0], "Result")
  | _ => none

/-- The field of the method with scheme `s` in the record of its class. -/
def mfield (s : Scheme) : Flick.Scheme :=
  ⟨s.k - 1, Facts.none, (trTT N (methTy [] s.ty)).substT (rot 1 (s.k - 1))⟩

def methsOf : Nat → List Nat
  | 0 => [0] | 1 => [1, 2] | 2 => [3] | 3 => [4] | 4 => [5] | _ => []

def methFs (C : Nat) : List (Label × Flick.Scheme) :=
  (methsOf C).filterMap fun m => (methSch m).map fun p => (methName m, mfield p.2)

/-- The fields of `D_C` with `n` parameters: the superclass slots, and the methods if `n = 1`. -/
def dfields (C n : Nat) : List (Label × Flick.Scheme) :=
  (supers C).map (fun Sup => ("slot", ⟨0, Facts.none, trD N (Sup.subst (params (vars n)))⟩)) ++
    (if n = 1 then methFs C else [])

def globF (g : Nat) : Option Flick.Scheme :=
  if g % 4 = 0 then (insts (g / 4)).map fun ins => ⟨0, Facts.none, trTT N (instTy ins)⟩
  else if g % 4 = 1 then
    (constSch (g / 4)).map fun s => ⟨s.k, Facts.none, trTT N (dicts .cnil s.ctx (.mono s.ty))⟩
  else if g = 2 then some ⟨0, Facts.none, trTT N (strctTy cEq tyRec [tyInt])⟩
  else none

theorem caps_v0 {σ : Atom → Flick.Ty} (h : σ (.v 0) = .tvar 0) :
    Caps.substT σ (fun a => a = .v 0) = fun a => a = .v 0 := by
  funext a; apply propext; simp [Caps.substT, h]

theorem ctorF_closed : ∀ K k ts T σ, ctorF K = some (k, ts, T) →
    (∀ i, i < k → σ (.v i) = .tvar i) → ts.map (Flick.Ty.substT σ) = ts := by
  intro K k ts T σ hK hσ
  unfold ctorF at hK
  split at hK <;> cases hK <;>
    simp [Flick.Ty.substT, effField, listT, seqT, lqT, hσ, caps_v0]

theorem ctorF_reach : ∀ K k ts T, ctorF K = some (k, ts, T) →
    ∀ t ∈ ts, ∀ a, t.reach a → ∃ i, i < k ∧ a = .v i := by
  intro K k ts T hK t ht a ha
  unfold ctorF at hK
  split at hK <;> cases hK <;> simp at ht <;>
    (try rcases ht with rfl | rfl) <;> (try subst ht) <;>
    simp [Flick.Ty.reach, effField, listT, seqT, lqT, Caps.none] at ha <;>
    (try rcases ha with ha | ha) <;> (try subst ha) <;> refine ⟨_, ?_, rfl⟩ <;> decide

theorem ty_lt {t : Ty} {k : Nat} (h : ∀ n, t.Occurs n → n < k) : ∀ n, t.Occurs n → n < k := h

theorem methSch_lt {m C s} (h : methSch m = some (C, s)) : ∀ n, s.ty.Occurs n → n < s.k := by
  unfold methSch at h
  split at h <;> cases h <;> intro n hn <;>
    simp [Ty.Occurs, Stage.Occurs, bindTy, map2Ty, appQ, arrP, boolTy] at hn ⊢ <;> omega

theorem dict_closed : ∀ c k fs, some (c / 8, dfields (c % 8) (c / 8)) = some (k, fs) →
    ∀ l s, (l, s) ∈ fs → s.Closed (s.k + k) := by
  intro c k fs h l s hs
  cases h
  simp only [dfields, List.mem_append, List.mem_map] at hs
  rcases hs with ⟨Sup, hS, he⟩ | hs
  · cases he
    unfold supers at hS
    split at hS <;> simp at hS <;> subst hS <;> apply trD_closed <;>
      simp [params, vars, Ty.subst] <;> intro n hn <;>
      (by_cases h0 : 0 < c / 8
       · simp [List.getElem?_range h0, Ty.Occurs] at hn; omega
       · simp [show c / 8 = 0 by omega, Ty.Occurs] at hn)
  · split at hs
    · rename_i h1
      simp only [methFs, List.mem_filterMap] at hs
      obtain ⟨m, -, hm⟩ := hs
      cases e : methSch m with
      | none => simp [e] at hm
      | some p =>
        simp [e] at hm; obtain ⟨-, rfl⟩ := hm
        rw [h1]
        have hk : 1 ≤ p.2.k := by
          unfold methSch at e; split at e <;> cases e <;> decide
        show Flick.Scheme.Closed (p.2.k - 1 + 1) _
        rw [Nat.add_comm]
        exact mfield_closed N fun n hn => by have := methSch_lt e n hn; omega
    · simp at hs

theorem vars_occ {n m : Nat} {t : Ty} (ht : t ∈ vars n) (h : t.Occurs m) : m < n := by
  simp only [vars, List.mem_map, List.mem_range] at ht
  obtain ⟨j, hj, rfl⟩ := ht; simp [Ty.Occurs] at h; omega

theorem curry_occ {q r : Ty} {m : Nat} : ∀ {as : List Ty}, (curry q as r).Occurs m →
    q.Occurs m ∨ (∃ a ∈ as, a.Occurs m) ∨ r.Occurs m
  | [], h => .inr (.inr h)
  | a :: as, h => by
    rcases h with h | h | h | h
    · exact .inr (.inl ⟨a, by simp, h⟩)
    · rcases curry_occ h with h | ⟨b, hb, h⟩ | h
      · exact .inl h
      · exact .inr (.inl ⟨b, by simp [hb], h⟩)
      · exact .inr (.inr h)
    · exact .inl h
    · cases h

theorem nest_occ {tup m : Nat} (as : List Ty) : ∀ {t : Ty}, (nest tup t as).Occurs m →
    t.Occurs m ∨ ∃ a ∈ as, a.Occurs m := by
  induction as with
  | nil => intro t h; exact .inl h
  | cons a as ih =>
    intro t h
    rcases ih h with h | ⟨b, hb, h⟩
    · simp only [Ty.Occurs, false_or] at h
      rcases h with h | h
      · exact .inl h
      · exact .inr ⟨a, by simp, h⟩
    · exact .inr ⟨b, by simp [hb], h⟩

theorem uncSch_lt (n : Nat) : ∀ m, (uncSch 10 n).ty.Occurs m → m < (uncSch 10 n).k := by
  intro m h
  simp only [uncSch, arrP] at h ⊢
  rcases h with h | (h | h | h | h) | h | h
  · rcases curry_occ h with h | ⟨a, ha, h⟩ | h
    · simp [Ty.Occurs] at h; omega
    · have := vars_occ ha h; omega
    · simp [Ty.Occurs] at h; omega
  · rcases nest_occ _ h with h | ⟨a, ha, h⟩
    · simp [Ty.Occurs] at h; omega
    · have := vars_occ (List.mem_of_mem_tail ha) h; omega
  · simp [Ty.Occurs] at h; omega
  · simp [Ty.Occurs] at h; omega
  · cases h
  · cases h
  · cases h

theorem constSch_ok {c s} (h : constSch c = some s) : s.ctx = [] ∧ ∀ n, s.ty.Occurs n → n < s.k := by
  unfold constSch at h
  split at h
  rotate_right
  · split at h
    · split at h
      · cases h; exact ⟨rfl, uncSch_lt _⟩
      · cases h
    · cases h; exact ⟨rfl, fun n hn => by simp [frSch, Ty.Occurs, Stage.Occurs] at hn ⊢; omega⟩
  all_goals
    first
    | (cases h; done)
    | (cases h
       refine ⟨rfl, ?_⟩
       intro n hn
       simp [Ty.Occurs, Stage.Occurs, bindTy, appQ, arrP, optTy] at hn ⊢
       try omega)

theorem glob_closed : ∀ g s, globF g = some s → s.Closed s.k := by
  intro g s h
  unfold globF at h
  split at h
  · cases e : insts (g / 4) with
    | none => simp [e] at h
    | some ins =>
      simp [e] at h; subst h
      unfold insts at e
      split at e <;> cases e <;> exact trD_closed N (by simp [tyInt, Ty.Occurs])
  · split at h
    · cases e : constSch (g / 4) with
      | none => simp [e] at h
      | some s' =>
        simp [e] at h; subst h
        obtain ⟨hc, hl⟩ := constSch_ok e
        simp only [hc]
        exact trT_closed N hl
    · split at h
      · cases h
        intro σ σ' _
        refine ⟨?_, by simp⟩
        simp [strctTy, dicts, trTT, trD, trT, tyInt, tyRec, cEq, Flick.Ty.substT, Flick.Ty.spine,
          Flick.Ty.reach, Flick.Stage.substT]
      · cases h

def Sg : Sig where
  ctor := ctorF
  impl _ := .rnil
  ctor_closed := ctorF_closed
  ctor_reach := ctorF_reach
  impl_closed _ _ := rfl
  dict c := some (c / 8, dfields (c % 8) (c / 8))
  glob := globF
  dict_closed := dict_closed
  glob_closed := glob_closed

end Flick.Elab.Witness
