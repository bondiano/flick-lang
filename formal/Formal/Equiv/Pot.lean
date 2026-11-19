import Formal.Equiv.Force

/-!
# Contextual equivalence: the work the left side does alone

A step of the left side that the right side answers by no step is one of the special pairs: it
makes a lazy cell the right side does not have, binds it, forces it, or settles it. Such steps
cannot go on forever (`docs/formal/lazy.md` §8): each lowers the potential `pot` of the thread,
which counts the forms these steps consume (`Tm.pot`), and the initializer of the cell a thread
has just made to force at its next step, which waits in the store (`xpot`). A thread related by
`Sim` forces no such cell at its next step (`Sim.xpot`), so only a thread that has just made one
(`OneShot`) has a potential in the store.
-/

namespace Flick.Equiv

variable {W : World}

theorem lazy_redex (e : Tm) : Redex (.lazy e) := by
  refine ⟨fun h => ?_, fun F a _ h => ?_⟩
  · cases h
  · cases F <;> simp [Frame.plug] at h

theorem let_redex {v b : Tm} (hv : Value v) : Redex (.let_ v b) := by
  refine ⟨fun h => ?_, fun F a _ h => ?_⟩
  · cases h
  · cases F <;> simp [Frame.plug] at h; exact h.1 ▸ hv

theorem force_redex (l : Nat) : Redex (.force (.loc l)) := by
  refine ⟨fun h => ?_, fun F a _ h => ?_⟩
  · cases h
  · cases F <;> simp [Frame.plug] at h; exact h ▸ .loc

theorem settle_redex {l : Nat} {v : Tm} (hv : Value v) : Redex (.settle l v) := by
  refine ⟨fun h => ?_, fun F a _ h => ?_⟩
  · cases h
  · cases F <;> simp [Frame.plug] at h; exact h.2 ▸ hv

theorem todo_redex : Redex .todo := by
  refine ⟨fun h => ?_, fun F a _ h => ?_⟩
  · cases h
  · cases F <;> simp [Frame.plug] at h

/-- A thread about to force a left cell, the initializer it waits with. -/
def Forces (W : World) (μ : List Tm) (t : Tm) (q : List Frame × Nat × Tm) : Prop :=
  OkE q.1 ∧ t = plug q.1 (.force (.loc q.2.1)) ∧ W.J₁ q.2.1 ∧ μ[q.2.1]? = some (.lazy q.2.2)

open Classical in
/-- The initializer a thread forces at its next step, of a cell only the left side has. -/
noncomputable def xpot (W : World) (μ : List Tm) (t : Tm) : Nat :=
  if h : ∃ q, Forces W μ t q then (Classical.choose h).2.2.pot else 0

/-- The potential of a thread of the left side. -/
noncomputable def pot (W : World) (μ : List Tm) (t : Tm) : Nat := t.pot + xpot W μ t

theorem Forces.eq {μ : List Tm} {t : Tm} {q q' : List Frame × Nat × Tm} (h : Forces W μ t q)
    (h' : Forces W μ t q') : q = q' := by
  obtain ⟨E, n, e⟩ := q
  obtain ⟨E', n', e'⟩ := q'
  obtain ⟨hE, rfl, -, he⟩ := h
  obtain ⟨hE', h₁, -, he'⟩ := h'
  obtain ⟨rfl, h₂⟩ := decomp_eq (force_redex n) (force_redex n') hE hE' h₁
  cases h₂
  rw [he] at he'; cases he'
  rfl

theorem xpot_eq {μ : List Tm} {t : Tm} {q : List Frame × Nat × Tm} (h : Forces W μ t q) :
    xpot W μ t = q.2.2.pot := by
  have hq : ∃ q, Forces W μ t q := ⟨q, h⟩
  rw [xpot, dif_pos hq, Forces.eq (Classical.choose_spec hq) h]

theorem xpot_zero {μ : List Tm} {t : Tm} (h : ∀ q, ¬ Forces W μ t q) : xpot W μ t = 0 := by
  rw [xpot, dif_neg (fun ⟨q, hq⟩ => h q hq)]

theorem xpot_le {W' : World} {μ μ' : List Tm} {t : Tm}
    (h : ∀ q, Forces W' μ' t q → Forces W μ t q) : xpot W' μ' t ≤ xpot W μ t := by
  by_cases hq : ∃ q, Forces W' μ' t q
  · obtain ⟨q, hq⟩ := hq
    rw [xpot_eq hq, xpot_eq (h q hq)]
    exact Nat.le_refl _
  · rw [xpot_zero (fun q h => hq ⟨q, h⟩)]; exact Nat.zero_le _

/-- A frame of a special pair holds no force. -/
theorem Act.not_force {a b : Tm} (h : Act W a b) (hB : ¬ W.B) {E : List Frame} {n : Nat}
    (hE : OkE E) (ha : a = plug E (.force (.loc n))) : False := by
  cases h with
  | dropA =>
    obtain ⟨-, h⟩ := decomp_eq (E := [.let_ _]) (lazy_redex _) (force_redex n)
      (OkE.nil.cons' trivial) hE ha
    cases h
  | dropB =>
    obtain ⟨-, h⟩ := decomp_eq (E := []) (let_redex .loc) (force_redex n) OkE.nil hE ha
    cases h
  | forceA =>
    obtain ⟨-, h⟩ := decomp_eq (E := [.force]) (lazy_redex _) (force_redex n)
      (OkE.nil.cons' trivial) hE ha
    cases h
  | bornA hB' | bornB hB' | bornS hB' | bornV hB' | dead hB' => exact hB hB'

/-- **A thread related by `Sim` forces no left cell at its next step**: the focus of a force of a
related cell is related by a congruence, and a related location is no left cell. -/
theorem Sim.no_force {μ μ₂ : List Tm} {t t₂ : Tm} (h : Sim W .any t t₂) (hst : StR W μ μ₂)
    (hB : ¬ W.B) {E : List Frame} {n : Nat} (hE : OkE E) (ht : t = plug E (.force (.loc n))) :
    ¬ W.J₁ n := by
  intro hn
  subst ht
  rcases h.dec (force_redex n).1 (fun _ _ h => by cases h) (fun _ _ _ h => by cases h)
      (fun h => h) _ _ rfl hE μ₂ with
    ⟨W', μ', E₀, E₁, a, b, E₂, hj, rfl, rfl, -, -, hact⟩ |
    ⟨W', μ', E₂, r₂, hj, -, -, hs⟩ | ⟨_, _, _, _, _, _, _, _, _, h', _, _⟩
  · exact hact.not_force (fun h => hB (hj.le.B.2 h)) (fun F hF => hE F (List.mem_append_left _ hF)) rfl
  · obtain ⟨e₂, rfl, he⟩ := hs.inv_force
    obtain ⟨W'', μ'', l', k, r, hl⟩ := force_loc he μ'
    exact (r.st _ (hj.st _ hst)).not_j₁ hl (r.le.J₁ _ (hj.le.J₁ _ hn))
  · cases h'

theorem Sim.xpot {μ μ₂ : List Tm} {t t₂ : Tm} (h : Sim W .any t t₂) (hst : StR W μ μ₂)
    (hB : ¬ W.B) : xpot W μ t = 0 :=
  xpot_zero fun ⟨_, _, _⟩ ⟨hE, ht, hn, _⟩ => h.no_force hst hB hE ht hn

end Flick.Equiv
