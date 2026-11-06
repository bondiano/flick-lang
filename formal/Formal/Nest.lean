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

structure Nest (Ψ : List Ty) (c : Cfg) (anc B : Nat → Nat → Prop)
    (home : Nat → Option (Nat × Nat)) : Prop where
  self : ∀ q, anc q q
  le : ∀ q x, anc q x → x ≤ q
  chain : ∀ q, Live c.rgs q → Chain anc (anc q)
  live : ∀ q x, Live c.rgs q → anc q x → Live c.rgs x
  thread : ∀ i e, c.at i = some e → e.Within Ψ B anc (baseOf anc i) ∧
    ∀ q, e.Has (.frame q) → home q = i ∧ Live c.rgs q
  home : ∀ q, Live c.rgs q → ∃ e, c.at (home q) = some e ∧ e.Has (.frame q)
  fins : ∀ (r : Nat) (g : Rgn), c.rgs[r]? = some g → g.live = true → ∀ f ∈ g.fins,
    f.Within Ψ B anc (anc r) ∧ ∀ q, ¬ f.Has (.frame q)
  cells : ∀ (l : Nat) (v : Tm), c.mem[l]? = some v → v.Within Ψ B anc (B l) ∧ v.NoIns

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

theorem Nest.base_chain {Ψ c anc B home i e} (hn : Nest Ψ c anc B home) (h : c.at i = some e) :
    Chain anc (baseOf anc i) := by
  cases i with
  | none => intro x hx; exact hx.elim
  | some rj => obtain ⟨r, j⟩ := rj; exact hn.chain r (Cfg.at_live h r j rfl)

/-- What a thread reaches through the store is live. -/
theorem Nest.reach {Ψ c anc B home i e p} (hn : Nest Ψ c anc B home) (h : c.at i = some e)
    (hr : Reaches c.mem e p) : Live c.rgs p := by
  obtain ⟨hw, hf⟩ := hn.thread i e h
  rcases hw.reaches hn.cells hr with h' | h'
  · exact baseOf_live hn.live (Cfg.at_live h) h'
  · exact (hf p h').2

/-- What a finalizer of a live region reaches through the store is live. -/
theorem Nest.reach_fin {Ψ c anc B home} {r : Nat} {g : Rgn} {f : Tm} {p : Nat} (hn : Nest Ψ c anc B home) (hg : c.rgs[r]? = some g)
    (hl : g.live = true) (hf : f ∈ g.fins) (hr : Reaches c.mem f p) : Live c.rgs p := by
  obtain ⟨hw, hni⟩ := hn.fins r g hg hl f hf
  rcases hw.reaches hn.cells hr with h' | h'
  · exact hn.live r p ⟨g, hg, hl⟩ h'
  · exact absurd h' (hni _)

/-- A thread does not use a region that is not live (T4.1). -/
theorem Nest.not_dead {Ψ c anc B home i E r} (hn : Nest Ψ c anc B home) (h : c.at i = some (plug E r))
    (hb : ∃ p, Dead c.rgs p ∧ (r = .app (.const .open_) (.own p) ∨ r = .app (.const .op) (.cap p) ∨
      (∃ t, r = .app (.app (.const .addFin) (.own p)) t) ∨
      (∃ t, r = .app (.app (.const .spawn) (.own p)) t) ∨ ∃ w, r = .inside p w ∧ Final w)) : False := by
  obtain ⟨p, hd, hr⟩ := hb
  have hp : r.Has (.rgn p) := by
    rcases hr with rfl | rfl | ⟨_, rfl⟩ | ⟨_, rfl⟩ | ⟨_, rfl, _⟩ <;> simp [Tm.Has]
  obtain ⟨g, hg, hl⟩ := hn.reach h (.here (plug_has E hp))
  rw [hd g hg] at hl; cases hl

/-- A finished computation at a place holds no `inside` form. -/
theorem Final.noFrame {Ψ B anc A} {w : Tm} (hw : Final w) (h : w.Within Ψ B anc A) (q : Nat) :
    ¬ w.Has (.frame q) := by
  rcases hw with hv | ⟨v, hv, rfl⟩ | ⟨p, ℓ, a, rfl⟩
  · exact (hv.noIns h).frame q
  · exact (hv.noIns (A := A) h).frame q
  · simpa [Tm.Has] using h.2.2.1 q

/-- The program at the start. -/
theorem Nest.init {Ψ e₀ t φ} (h : Typ S [] Facts.none [] e₀ (.ex t φ)) (hb : e₀.Below 0) :
    Nest Ψ ⟨[], [], e₀⟩ (fun q x => x = q) (fun _ _ => False) (fun _ => none) := by
  have hn : e₀.NoIns := hb.noIns
  have none : ∀ q, ¬ Live [] q := fun q ⟨_, hg, _⟩ => by simp at hg
  refine ⟨fun _ => rfl, fun _ _ h => h ▸ Nat.le_refl _, fun q h => (none q h).elim,
    fun q _ h => (none q h).elim, fun i e hi => ?_, fun q h => (none q h).elim,
    fun _ _ hg => by simp at hg, fun _ _ h => by simp at h⟩
  cases i with
  | none =>
    cases hi
    exact ⟨Tm.Within.of_has hn (fun p hp => absurd (hb p (.inl hp)) (Nat.not_lt_zero _))
      (fun l hl => absurd (h.locs l hl) (Nat.not_lt_zero _)), fun q hq => (hn.frame q hq).elim⟩
  | some rj => obtain ⟨g, hg, -⟩ := Cfg.at_some.mp hi; simp at hg

end Flick
