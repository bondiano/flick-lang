import Formal.WithinChain

/-!
# The threads of a configuration (regions.md §3.4, §5)

The threads of a configuration are the main thread and the children of the live
regions; `baseOf` is the place of a thread given the ancestors `anc` of the regions. The
invariant over them that T4′ uses is `LNest` (`Live/LNest*.lean`).
-/

namespace Flick

variable {S : Sig}

/-- The threads of a configuration: the main thread `none` and the child `j` of
the live region `ϱ`, `some (ϱ, j)`. -/
def Cfg.at (c : Cfg) : Option (Nat × Nat) → Option Tm
  | none => some c.main
  | some (r, j) => match c.rgs[r]? with
    | some g => if g.live then g.kids[j]? else none
    | none => none

/-- Replace a thread. -/
def Cfg.put (c : Cfg) : Option (Nat × Nat) → Tm → Cfg
  | none, e => ⟨c.rgs, c.mem, e⟩
  | some (r, j), e => ⟨setKid c.rgs r j e, c.mem, c.main⟩

/-- The regions a thread may name outside its `inside` forms. -/
def baseOf (anc : Nat → Nat → Prop) : Option (Nat × Nat) → Nat → Prop
  | none => fun _ => False
  | some (r, _) => anc r

theorem Cfg.at_some {c : Cfg} {r j : Nat} {k : Tm} :
    c.at (some (r, j)) = some k ↔ ∃ g, c.rgs[r]? = some g ∧ g.live = true ∧ g.kids[j]? = some k := by
  simp only [Cfg.at]
  cases c.rgs[r]? with
  | none => simp
  | some g => by_cases hl : g.live <;> simp [hl]

theorem Live.lt {Λ : List Rgn} {p : Nat} (h : Live Λ p) : p < Λ.length :=
  let ⟨_, hg, _⟩ := h; getElem?_lt hg

theorem baseOf_live {Λ : List Rgn} {anc : Nat → Nat → Prop} {i x}
    (hl : ∀ q x, Live Λ q → anc q x → Live Λ x)
    (hi : ∀ r j, i = some (r, j) → Live Λ r) (h : baseOf anc i x) : Live Λ x := by
  cases i with
  | none => exact h.elim
  | some rj => obtain ⟨r, j⟩ := rj; exact hl r x (hi r j rfl) h

theorem Cfg.at_live {c : Cfg} {i e} (h : c.at i = some e) : ∀ r j, i = some (r, j) → Live c.rgs r := by
  rintro r j rfl
  obtain ⟨g, hg, hl, -⟩ := Cfg.at_some.mp h
  exact ⟨g, hg, hl⟩

end Flick
