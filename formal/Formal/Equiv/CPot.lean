import Formal.Equiv.Slot

/-!
# Contextual equivalence: the potential of a configuration

The potential of a configuration of the left side (`cpot`) is that of its main thread and of
every child of its regions (`Formal/Equiv/Pot.lean`). A step that the right side answers by no
step lowers it: the potential of the thread that steps drops (`Zero`), and that of no other thread
grows (`ThR.xpot_le`). A thread related by `ThR` forces at its next step only a cell reserved for
it (`ThR.forces`); the cells reserved for other threads stay pending (`KeepO`), and a step answered
by none reserves new cells only for its own thread.
-/

namespace Flick.Equiv

variable {W : World}

/-- A related thread forces at its next step only the left cell reserved for it, pending. -/
theorem ThR.forces {μ μ₂ : List Tm} {τ : Option (Nat × Nat)} {t t₂ : Tm} (h : ThR W μ τ t t₂)
    (hst : StR W μ μ₂) (hB : ¬ W.B) {E : List Frame} {n : Nat} (hE : OkE E)
    (ht : t = plug E (.force (.loc n))) (hn : W.J₁ n) : W.O n τ ∧ ∃ e, μ[n]? = some (.lazy e) := by
  rcases h with h | ⟨E', E₂, m, e, e', rfl, -, hE', -, -, ho, hc, -⟩
  · exact (h.no_force hst hB hE ht hn).elim
  · obtain ⟨-, h⟩ := decomp_eq (force_redex m) (force_redex n) hE' hE ht
    cases h
    exact ⟨ho, e, hc⟩

/-- **No other thread's potential grows** at a step answered by no step: the cell it forces next
was reserved for it, and stays pending with the same initializer. -/
theorem ThR.xpot_le {W' : World} {μ μ' μ₂ μ₂' : List Tm} {τ τ₀ : Option (Nat × Nat)}
    {t t₂ t₂' : Tm} (h : ThR W μ τ₀ t t₂) (h' : ThR W' μ' τ₀ t t₂') (hst : StR W μ μ₂)
    (hst' : StR W' μ' μ₂') (hB : ¬ W.B) (hW : W.le W') (hko : KeepO W τ μ μ') (hτ : τ₀ ≠ τ)
    (ho : ∀ n τ', τ' ≠ τ → W'.O n τ' → W.O n τ') : xpot W' μ' t ≤ xpot W μ t := by
  refine Equiv.xpot_le ?_
  rintro ⟨E, n, e⟩ ⟨hE, ht, hn, he⟩
  have ho₀ := ho n τ₀ hτ (h'.forces hst' (fun h => hB (hW.B.2 h)) hE ht hn).1
  obtain ⟨-, e₀, he₀⟩ := h.forces hst hB hE ht (hst.own _ _ ho₀)
  have := hko n τ₀ e₀ hτ ho₀ he₀
  simp only at he
  rw [he] at this; cases this
  exact ⟨hE, ht, hst.own _ _ ho₀, he₀⟩

theorem pot_todo (μ : List Tm) : pot W μ .todo = 0 := by
  have : xpot W μ .todo = 0 := xpot_zero fun ⟨E, n, e⟩ ⟨_, ht, _, _⟩ => by
    rcases snoc_or E with rfl | ⟨E₀, F, rfl⟩
    · cases ht
    · rw [plug_snoc] at ht; cases F <;> cases ht
  simp [pot, this, Tm.pot]

/-- The sum of `f` over the children of the regions. -/
def ksum (f : Tm → Nat) (Λ : List Rgn) : Nat := (Λ.map fun g => (g.kids.map f).sum).sum

/-- The potential of a configuration of the left side. -/
noncomputable def cpot (W : World) (c : Cfg) : Nat := pot W c.mem c.main + ksum (pot W c.mem) c.rgs

theorem sum_set {α : Type} (f : α → Nat) :
    ∀ {l : List α} {i : Nat} {a : α} (b : α), l[i]? = some a →
      ((l.set i b).map f).sum + f a = (l.map f).sum + f b
  | [], _, _, _, h => by simp at h
  | x :: l, 0, a, b, h => by simp at h; subst h; simp; omega
  | x :: l, i + 1, a, b, h => by
    simp only [List.getElem?_cons_succ] at h
    have := sum_set f b h
    simp only [List.set_cons_succ, List.map_cons, List.sum_cons]
    omega

theorem sum_le {α : Type} {f f' : α → Nat} :
    ∀ {l : List α}, (∀ a ∈ l, f a ≤ f' a) → (l.map f).sum ≤ (l.map f').sum
  | [], _ => by simp
  | x :: l, h => by
    have h₁ := h x (List.mem_cons_self ..)
    have h₂ := sum_le (l := l) fun a ha => h a (List.mem_cons_of_mem _ ha)
    simp; omega

/-- The child `j` of the region `p` replaced. -/
theorem ksum_set (f : Tm → Nat) {Λ : List Rgn} {p j : Nat} {g : Rgn} {k : Tm} (k' : Tm)
    (hg : Λ[p]? = some g) (hk : g.kids[j]? = some k) :
    ksum f (setKid Λ p j k') + f k = ksum f Λ + f k' := by
  have e : setKid Λ p j k' = Λ.set p ⟨g.live, g.kids.set j k', g.fins⟩ := by
    unfold setKid setRgn; rw [hg]
  have h₁ := sum_set (fun g : Rgn => (g.kids.map f).sum) ⟨g.live, g.kids.set j k', g.fins⟩ hg
  have h₂ := sum_set f k' hk
  simp only [ksum, e]
  simp only at h₁
  omega

theorem ksum_le {f f' : Tm → Nat} {Λ : List Rgn}
    (h : ∀ (p : Nat) (g : Rgn) (j : Nat) (k : Tm), Λ[p]? = some g → g.kids[j]? = some k → f k ≤ f' k) :
    ksum f Λ ≤ ksum f' Λ := by
  apply sum_le
  intro g hg
  obtain ⟨p, hp⟩ := List.mem_iff_getElem?.mp hg
  apply sum_le
  intro k hk
  obtain ⟨j, hj⟩ := List.mem_iff_getElem?.mp hk
  exact h p g j k hp hj

/-- The children of the regions but the child `j` of the region `p`, which a neutral term holds. -/
theorem ksum_slot_le {f f' : Tm → Nat} {Λ : List Rgn} {p j : Nat} (hz : f .todo ≤ f' .todo)
    (h : ∀ (q : Nat) (g : Rgn) (i : Nat) (k : Tm), Λ[q]? = some g → g.kids[i]? = some k →
      (q, i) ≠ (p, j) → f k ≤ f' k) :
    ksum f (setKid Λ p j .todo) ≤ ksum f' (setKid Λ p j .todo) := by
  apply ksum_le
  intro q g i k hg hk
  rw [setKid_getElem] at hg
  obtain ⟨g₀, hg₀, rfl⟩ := Option.map_eq_some_iff.mp hg
  by_cases hq : q = p
  · subst hq
    simp only [Rgn.slot, if_true] at hk
    by_cases hi : i = j
    · subst hi
      rw [List.getElem?_set_self'] at hk
      obtain ⟨_, -, rfl⟩ := Option.map_eq_some_iff.mp hk
      exact hz
    · rw [List.getElem?_set_ne (Ne.symm hi)] at hk
      exact h q g₀ i k hg₀ hk (by simp [hi])
  · rw [Rgn.slot_ne _ hq] at hk
    exact h q g₀ i k hg₀ hk (by simp [hq])

end Flick.Equiv
