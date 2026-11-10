import Formal.WithinChain

/-!
# `⊢ Λ`: what live work names (regions.md §3.4, §5)

The threads of a configuration are the main thread and the children of the live
regions. `Nest` relates a configuration to the ancestors `anc` of its regions, the
regions `B ℓ` each cell may hold, and the thread `home π` whose `inside π` form
is that of `π`:

- the ancestors of a region are earlier regions, and those of a live region are
  live and nested (`Chain`); this is L5: a region closes after every region
  created in it;
- every thread is at the place of its base: the main thread names no region, a
  child of `ϱ` names the ancestors of `ϱ`; the `inside` forms of a thread are
  those of live regions whose home it is, and every live region has its form;
- finalizers are at the place of their region, cells at their `B ℓ`.

Hence every thread and finalizer reaches through the store only live regions
(`Nest.reach`): T4.
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
