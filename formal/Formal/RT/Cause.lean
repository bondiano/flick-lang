/-!
# λRT: the algebra of causes (runtime.md §1.1, §2.1–§2.2)

A cause is a flat list of reasons, as in Effect 4.0.5. `c ⊕ c'` is `nub (c ⧺ c')`
(`causeCombine`), with an empty argument returned unchanged; `c ⧺ c'` is list
concatenation (`causeFromReasons`). `≈` of §2.1 is equality of reasons: `V` stands for
the values Flick puts into a cause, with a thrown `Error` told apart by its identity.

This file imports nothing from the core of P1.
-/

namespace Flick.RT

/-- A reason (runtime.md §1.1): an expected error, a defect, an interruption by a fiber. -/
inductive Reason (V : Type) where
  | fail (v : V)
  | die (d : V)
  | int (f : Option Nat)
  deriving DecidableEq

/-- A cause is the list of its reasons. -/
abbrev Cause (V : Type) := List (Reason V)

section Nub

variable {α : Type} [DecidableEq α]

/-- `nubFrom s l`: the first occurrence of each element of `l` that is not in `s`. -/
def nubFrom : List α → List α → List α
  | _, [] => []
  | s, a :: l => if a ∈ s then nubFrom s l else a :: nubFrom (a :: s) l

/-- `nub` of §2.1 (`dedupeReasons`). -/
def nub (l : List α) : List α := nubFrom [] l

theorem nubFrom_mem {s : List α} {a : α} (h : a ∈ s) (l : List α) :
    nubFrom s (a :: l) = nubFrom s l := by simp [nubFrom, h]

theorem nubFrom_not {s : List α} {a : α} (h : a ∉ s) (l : List α) :
    nubFrom s (a :: l) = a :: nubFrom (a :: s) l := by simp [nubFrom, h]

theorem mem_nubFrom {x : α} {s l : List α} : x ∈ nubFrom s l ↔ x ∈ l ∧ x ∉ s := by
  induction l generalizing s with
  | nil => simp [nubFrom]
  | cons a l ih =>
    by_cases ha : a ∈ s
    · rw [nubFrom_mem ha, ih]; grind
    · rw [nubFrom_not ha, List.mem_cons, ih]; grind

theorem nubFrom_congr {s t : List α} (h : ∀ x, x ∈ s ↔ x ∈ t) (l : List α) :
    nubFrom s l = nubFrom t l := by
  induction l generalizing s t with
  | nil => rfl
  | cons a l ih =>
    by_cases ha : a ∈ s
    · rw [nubFrom_mem ha, nubFrom_mem ((h a).1 ha)]; exact ih h
    · rw [nubFrom_not ha, nubFrom_not (fun h' => ha ((h a).2 h'))]
      exact congrArg _ (ih fun x => by simp [List.mem_cons, h x])

theorem nubFrom_append (s l m : List α) :
    nubFrom s (l ++ m) = nubFrom s l ++ nubFrom (l ++ s) m := by
  induction l generalizing s with
  | nil => rfl
  | cons a l ih =>
    rw [List.cons_append]
    by_cases ha : a ∈ s
    · rw [nubFrom_mem ha, nubFrom_mem ha, ih]
      exact congrArg _ (nubFrom_congr (fun x => by simp only [List.mem_append, List.mem_cons]; grind) m)
    · rw [nubFrom_not ha, nubFrom_not ha, ih, List.cons_append]
      exact congrArg _ (congrArg _ (nubFrom_congr (fun x => by
        simp only [List.mem_append, List.mem_cons]; grind) m))

theorem nubFrom_nubFrom {s t : List α} (h : ∀ x ∈ t, x ∈ s) (l : List α) :
    nubFrom s (nubFrom t l) = nubFrom s l := by
  induction l generalizing s t with
  | nil => rfl
  | cons a l ih =>
    by_cases ha : a ∈ t
    · rw [nubFrom_mem ha, nubFrom_mem (h a ha)]; exact ih h
    · rw [nubFrom_not ha]
      by_cases hs : a ∈ s
      · rw [nubFrom_mem hs, nubFrom_mem hs]
        exact ih fun x hx => by
          rcases List.mem_cons.1 hx with rfl | hx
          · exact hs
          · exact h x hx
      · rw [nubFrom_not hs, nubFrom_not hs]
        exact congrArg _ (ih fun x hx => by
          rcases List.mem_cons.1 hx with rfl | hx
          · exact List.mem_cons_self
          · exact List.mem_cons_of_mem _ (h x hx))

theorem nubFrom_all {s l : List α} (h : ∀ x ∈ l, x ∈ s) : nubFrom s l = [] := by
  induction l with
  | nil => rfl
  | cons a l ih =>
    rw [nubFrom_mem (h a List.mem_cons_self)]
    exact ih fun x hx => h x (List.mem_cons_of_mem _ hx)

theorem mem_nub {x : α} {l : List α} : x ∈ nub l ↔ x ∈ l := by
  simp [nub, mem_nubFrom]

theorem nub_nub (l : List α) : nub (nub l) = nub l :=
  nubFrom_nubFrom (fun _ h => h) l

theorem nub_append (l m : List α) : nub (l ++ m) = nub l ++ nubFrom l m := by
  rw [nub, nubFrom_append, List.append_nil]; rfl

theorem nub_nub_append (l m : List α) : nub (nub l ++ m) = nub (l ++ m) := by
  rw [nub_append, nub_nub, nub_append]
  exact congrArg _ (nubFrom_congr (fun _ => mem_nub) m)

theorem nub_append_nub (l m : List α) : nub (l ++ nub m) = nub (l ++ m) := by
  rw [nub_append, nub_append, nub]
  exact congrArg _ (nubFrom_nubFrom (fun _ h => by simp at h) m)

end Nub

variable {V : Type} [DecidableEq V]

/-- `c ⊕ c'` of §2.1 (`causeCombine`): an empty side returns the other unchanged. -/
def comb (a b : Cause V) : Cause V :=
  if a = [] then b else if b = [] then a else nub (a ++ b)

/-- The normal form of §2.2: a list without repeats. -/
def NF (c : Cause V) : Prop := nub c = c

theorem nf_nub (c : Cause V) : NF (nub c) := nub_nub c

theorem nf_nil : NF ([] : Cause V) := rfl

theorem comb_nf {a b : Cause V} (ha : NF a) (hb : NF b) : comb a b = nub (a ++ b) := by
  unfold comb
  by_cases h₁ : a = []
  · subst h₁; simp only [if_true, List.nil_append]; exact hb.symm
  · by_cases h₂ : b = []
    · subst h₂; simp only [h₁, if_true, if_false, List.append_nil]; exact ha.symm
    · simp [h₁, h₂]

theorem nf_comb {a b : Cause V} (ha : NF a) (hb : NF b) : NF (comb a b) := by
  rw [comb_nf ha hb]; exact nf_nub _

/-- (⊕-Unit). -/
theorem comb_nil_left (c : Cause V) : comb [] c = c := by simp [comb]

theorem comb_nil_right (c : Cause V) : comb c [] = c := by
  unfold comb; by_cases h : c = [] <;> simp [h]

/-- (⊕-Assoc) on normal forms. -/
theorem comb_assoc {a b c : Cause V} (ha : NF a) (hb : NF b) (hc : NF c) :
    comb (comb a b) c = comb a (comb b c) := by
  rw [comb_nf (nf_comb ha hb) hc, comb_nf ha (nf_comb hb hc), comb_nf ha hb, comb_nf hb hc,
    nub_nub_append, nub_append_nub, List.append_assoc]

/-- (⊕-Idem): `c ⊕ c = c`. -/
theorem comb_self {c : Cause V} (hc : NF c) : comb c c = c := by
  rw [comb_nf hc hc, nub_append, nubFrom_all (fun _ h => h), List.append_nil]; exact hc

/-- (⊕-Idem): `a ⊕ b ⊕ a = a ⊕ b`. -/
theorem comb_absorb {a b : Cause V} (ha : NF a) (hb : NF b) :
    comb (comb a b) a = comb a b := by
  rw [comb_nf (nf_comb ha hb) ha, comb_nf ha hb, nub_nub_append, nub_append (a ++ b) a,
    nubFrom_all (fun _ h => List.mem_append.2 (.inl h)), List.append_nil]

/-- `head(c)` of §2.2: the first `Fail v`. -/
def head : List (Reason V) → Option V
  | [] => none
  | Reason.fail v :: _ => some v
  | _ :: c => head c

omit [DecidableEq V] in
theorem head_append (a b : Cause V) :
    head (a ++ b) = match head a with | some v => some v | none => head b := by
  induction a with
  | nil => rfl
  | cons r a ih => cases r <;> simp [head, ih]

/-- A list whose reasons are no `Fail` keeps the head of what follows. -/
def NoFail (s : Cause V) : Prop := ∀ v, Reason.fail v ∉ s

theorem head_nubFrom {s : Cause V} (hs : NoFail s) (l : Cause V) :
    head (nubFrom s l) = head l := by
  induction l generalizing s with
  | nil => rfl
  | cons a l ih =>
    by_cases ha : a ∈ s
    · rw [nubFrom_mem ha, ih hs]
      cases a with
      | fail v => exact absurd ha (hs v)
      | die _ | int _ => rfl
    · rw [nubFrom_not ha]
      cases a with
      | fail v => rfl
      | die d | int d =>
        refine ih fun v hv => ?_
        rcases List.mem_cons.1 hv with h | h
        · cases h
        · exact hs v h

theorem head_nub (l : Cause V) : head (nub l) = head l := head_nubFrom (fun _ h => by simp at h) l

/-- (⊕-Head), for every pair of lists. -/
theorem head_comb (a b : Cause V) :
    head (comb a b) = match head a with | some v => some v | none => head b := by
  unfold comb
  by_cases h₁ : a = []
  · subst h₁; simp [head]
  · by_cases h₂ : b = []
    · subst h₂; simp only [h₁, if_true, if_false]; cases head a <;> rfl
    · simp only [h₁, h₂, if_false, head_nub, head_append]

/-- A cause as Go stores it: a binary tree read by `reasons()` (`cause.go:11-61`). -/
inductive CTree (V : Type) where
  | leaf (c : Cause V)
  | seq (a b : CTree V)
  | par (a b : CTree V)

/-- `reasons()`: both node kinds combine with `⊕` (Р30). -/
def CTree.reasons : CTree V → Cause V
  | .leaf c => c
  | .seq a b => comb a.reasons b.reasons
  | .par a b => comb a.reasons b.reasons

/-- **R8** (runtime.md §2.2, §4): the laws of `⊕` and `⧺` on normal forms. `pretty` and
`report` of §2.5 are functions of `Cause V`, the list, so they cannot see more; the Go
tree is read through `CTree.reasons`. -/
theorem R8 :
    (∀ c : Cause V, comb [] c = c ∧ comb c [] = c) ∧
    (∀ a b c : Cause V, NF a → NF b → NF c → comb (comb a b) c = comb a (comb b c)) ∧
    (∀ a b : Cause V, NF a → NF b → comb a a = a ∧ comb (comb a b) a = comb a b) ∧
    (∀ a b : Cause V, head (comb a b) = match head a with | some v => some v | none => head b) ∧
    (∀ a b : CTree V, (CTree.seq a b).reasons = (CTree.par a b).reasons) ∧
    (∀ a b c : Cause V, [] ++ a = a ∧ a ++ [] = a ∧ (a ++ b) ++ c = a ++ (b ++ c)) ∧
    (∀ a b : Cause V, NF a → NF b → NF (comb a b)) :=
  ⟨fun c => ⟨comb_nil_left c, comb_nil_right c⟩,
   fun _ _ _ => comb_assoc,
   fun _ _ ha hb => ⟨comb_self ha, comb_absorb ha hb⟩,
   head_comb,
   fun _ _ => rfl,
   fun a b c => ⟨List.nil_append a, List.append_nil a, List.append_assoc a b c⟩,
   fun _ _ => nf_comb⟩

end Flick.RT
