import Formal.Covers
import Formal.Closed

/-!
# What patterns bind, and cells of a store typing
-/

namespace Flick

variable {S : Sig}

theorem PatTy.binds {fl p t Δ} (h : PatTy S fl p t Δ) : Δ.length = p.binds := by
  induction h <;> simp_all [Pat.binds]

theorem Pat.spine_binds (K : Name) (qs : List Pat) :
    (Pat.spine K qs).binds = (qs.map Pat.binds).sum := by
  have key : ∀ (qs : List Pat) (p : Pat), (qs.foldl .app p).binds = p.binds + (qs.map Pat.binds).sum := by
    intro qs
    induction qs with
    | nil => intro p; simp
    | cons q qs ih => intro p; simp only [List.foldl, ih, Pat.binds, List.map_cons, List.sum_cons]; omega
  simpa [Pat.spine, Pat.binds] using key qs (.ctor K)

theorem PatsTy.binds {qs us Δ} (h : PatsTy S qs us Δ) : Δ.length = (qs.map Pat.binds).sum := by
  induction h with
  | nil => rfl
  | cons hq _ ih => simp [hq.binds, ih]

/-- The arm of a GADT binds what its patterns bind, under any instance. -/
theorem PatsTy.spine_binds {K qs us Δ} (h : PatsTy S qs us Δ) :
    (Δ.map Scheme.mono).length = (Pat.spine K qs).binds := by
  simp [h.binds, Pat.spine_binds]

theorem getElem?_lt {α} {l : List α} {i : Nat} {a : α} (h : l[i]? = some a) : i < l.length := by
  obtain ⟨hi, _⟩ := List.getElem?_eq_some_iff.mp h
  exact hi

theorem getElem?_ext {α} {l l' : List α} {i : Nat} {a : α} (h : l[i]? = some a) :
    (l ++ l')[i]? = some a := by
  rw [List.getElem?_append_left (getElem?_lt h)]; exact h

end Flick
