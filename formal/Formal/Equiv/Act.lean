import Formal.Equiv.Simple
import Formal.Equiv.Pot

/-!
# Contextual equivalence: steps of the left side of a special pair

The left side of a special pair (`Act`) steps by its own redex while the right side waits, or,
for `lazy v` against `born v'` (O1b), runs `born v'` to its value (`act_sim`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

/-- The cells other threads are about to force stay pending. -/
def KeepO (W : World) (τ : Option (Nat × Nat)) (μ μ' : List Tm) : Prop :=
  ∀ n τ' e, τ' ≠ τ → W.O n τ' → μ[n]? = some (.lazy e) → μ'[n]? = some (.lazy e)

theorem KeepO.of_junk {τ : Option (Nat × Nat)} {μ μ' μ₂ : List Tm} (hst : StR W μ μ₂)
    (h : ∀ n, W.J₁ n → μ'[n]? = μ[n]?) : KeepO W τ μ μ' :=
  fun n _ _ _ ho he => (h n (hst.own _ _ ho)).trans he

theorem KeepO.append {τ : Option (Nat × Nat)} {μ μ₂ : List Tm} (hst : StR W μ μ₂) (c : Tm) :
    KeepO W τ μ (μ ++ [c]) :=
  KeepO.of_junk hst fun n hn => getElem?_append_old (hst.j₁ n hn).1

/-- An answer by no step to the step `t → t'` of the thread `τ`: without the pairs of O1b, the
potential of the thread drops, and only it reserves new cells. -/
def Zero (W W' : World) (τ : Option (Nat × Nat)) (μ μ' : List Tm) (t t' : Tm) : Prop :=
  ¬ W.B → pot W' μ' t' < pot W μ t ∧ ∀ n τ', τ' ≠ τ → W'.O n τ' → W.O n τ'

theorem Zero.of_B {W' : World} {τ : Option (Nat × Nat)} {μ μ' : List Tm} {t t' : Tm} (h : W.B) :
    Zero W W' τ μ μ' t t' := fun hn => (hn h).elim

theorem xpot_congr {W' : World} (h : ∀ n, W'.J₁ n ↔ W.J₁ n) (μ : List Tm) (t : Tm) :
    xpot W' μ t = xpot W μ t := by
  apply Nat.le_antisymm <;> apply xpot_le <;> intro ⟨E, n, e⟩ ⟨hE, ht, hn, he⟩
  · exact ⟨hE, ht, (h n).1 hn, he⟩
  · exact ⟨hE, ht, (h n).2 hn, he⟩

/-- An answer of no step after the right side caught up without steps. -/
theorem Zero.pre {W₁ W' : World} {τ : Option (Nat × Nat)} {μ μ' μ₂ μ₂' : List Tm} {t t' t₂ t₂' : Tm}
    (hj : RJ W μ₂ t₂ W₁ μ₂' t₂') (h : Zero W₁ W' τ μ μ' t t') : Zero W W' τ μ μ' t t' := by
  intro hB
  obtain ⟨h₁, h₂⟩ := h (fun h => hB (hj.le.B.2 h))
  have e : pot W₁ μ t = pot W μ t := by
    simp only [pot]
    rw [xpot_congr (fun n => ⟨hj.j₁ n, hj.le.J₁ n⟩)]
  exact ⟨e ▸ h₁, fun n τ' hτ ho => hj.o _ _ (h₂ n τ' hτ ho)⟩

/-- An answer of no step whose result is related by `Sim`: it forces no left cell next. -/
theorem Zero.sim {W' : World} {τ : Option (Nat × Nat)} {μ μ' μ₂' : List Tm} {t t' t₂' : Tm}
    (hr : Sim W' .any t' t₂') (hst : StR W' μ' μ₂') (hB : W.le W') (hlt : t'.pot < t.pot)
    (ho : ∀ n τ', τ' ≠ τ → W'.O n τ' → W.O n τ') : Zero W W' τ μ μ' t t' := fun hn => by
  refine ⟨?_, ho⟩
  have := Sim.xpot hr hst (fun h => hn (hB.B.2 h))
  simp only [pot]
  omega

/-- The right side's answer to the step `t → t'` of the thread `τ` that keeps the regions: at
least one step, or none (`Zero`). -/
def KAns (root : Roots) (W : World) (τ : Option (Nat × Nat)) (μ₂ : List Tm)
    (t₂ : Tm) (μ μ' : List Tm) (t t' : Tm) : Prop :=
  ∃ W' μ₂' t₂', W.le W' ∧
    (KPos root μ₂ t₂ μ₂' t₂' ∨ (μ₂' = μ₂ ∧ t₂' = t₂ ∧ Zero W W' τ μ μ' t t')) ∧
    ThR W' μ' τ t' t₂' ∧ StR W' μ' μ₂' ∧ KeepO W τ μ μ'

/-- Steps of the right side that keep the regions, when the pairs of O1b may stand. -/
theorem KSteps.ans {root : Roots} {W' : World} {τ : Option (Nat × Nat)} {μ μ' μ₂ μ₂' : List Tm}
    {t t' t₂ t₂' : Tm} (h : KSteps root μ₂ t₂ μ₂' t₂') (hB : W.B) :
    KPos root μ₂ t₂ μ₂' t₂' ∨ (μ₂' = μ₂ ∧ t₂' = t₂ ∧ Zero W W' τ μ μ' t t') := by
  rcases h.split with ⟨rfl, rfl⟩ | h
  · exact .inr ⟨rfl, rfl, Zero.of_B hB⟩
  · exact .inl h

theorem KAns.ksteps {root : Roots} {W' : World} {τ : Option (Nat × Nat)} {μ μ' μ₂ μ₂' : List Tm}
    {t t' t₂ t₂' : Tm}
    (h : KPos root μ₂ t₂ μ₂' t₂' ∨ (μ₂' = μ₂ ∧ t₂' = t₂ ∧ Zero W W' τ μ μ' t t')) :
    KSteps root μ₂ t₂ μ₂' t₂' := by
  rcases h with h | ⟨rfl, rfl, -⟩
  · exact h.ksteps
  · exact .refl

theorem plug_noframe {E : List Frame} {r x : Tm} (hx : ∀ (F : Frame) (a : Tm), F.plug a ≠ x)
    (h : plug E r = x) : r = x := by
  rcases snoc_or E with rfl | ⟨E₀, F, rfl⟩
  · exact h
  · rw [plug_snoc] at h; exact (hx _ _ h).elim

theorem plug_value {E : List Frame} {r v : Tm} (hr : ¬ Value r) (hv : Value v) (h : plug E r = v) :
    False := not_value_plugs hr E (h ▸ hv)

/-- The focus of a step in the left side of a special pair is a form of a lazy cell: one a step of
a context does not take. -/
theorem Act.simple {μ μ₂ : List Tm} (hst : StR W μ μ₂) {a b : Tm} (h : Act W a b)
    {E : List Frame} {r : Tm} (hr : ¬ Value r) (ha : a = plug E r) : r.Simple := by
  have nl : ∀ {e : Tm} (F : Frame) (x : Tm), F.plug x ≠ .lazy e := fun F _ h => by
    cases F <;> simp [Frame.plug] at h
  have nt : ∀ (F : Frame) (x : Tm), F.plug x ≠ .todo := fun F _ h => by
    cases F <;> simp [Frame.plug] at h
  have val : ∀ {l l' v v'}, W.L l l' (.born v v') → Value v := fun hl => by
    obtain ⟨c, c', -, -, hi⟩ := hst.cell _ _ _ hl
    exact (show BornInv W _ _ c c' from hi).val
  replace ha := ha.symm
  rcases snoc_or E with rfl | ⟨E₀, F, rfl⟩
  · simp only [plug] at ha
    subst ha
    cases h with
    | bornS _ _ _ hs => rcases hs with rfl | rfl | rfl | rfl | rfl <;> trivial
    | _ => trivial
  rw [plug_snoc] at ha
  cases h with
  | dropA | bornB =>
    cases F <;> simp only [Frame.plug, reduceCtorEq, Tm.let_.injEq] at ha
    rw [plug_noframe nl ha.1]; trivial
  | dropB =>
    cases F <;> simp only [Frame.plug, reduceCtorEq, Tm.let_.injEq] at ha
    exact (plug_value hr .loc ha.1).elim
  | forceA =>
    cases F <;> simp only [Frame.plug, reduceCtorEq, Tm.force.injEq] at ha
    rw [plug_noframe nl ha]; trivial
  | bornA => exact (nl _ _ ha).elim
  | dead => exact (nt _ _ ha).elim
  | bornV _ hl =>
    cases F <;> simp only [Frame.plug, reduceCtorEq, Tm.settle.injEq] at ha
    exact (plug_value hr (val hl) ha.2).elim
  | bornS _ hl _ hs =>
    rcases hs with rfl | rfl | rfl | rfl | rfl <;>
      cases F <;> simp only [Frame.plug, reduceCtorEq, Tm.let_.injEq] at ha
    · exact (plug_value hr .loc ha.1).elim
    · rcases snoc_or E₀ with rfl | ⟨E₁, G, rfl⟩
      · simp only [plug] at ha; rw [ha.1]; trivial
      · rw [plug_snoc] at ha
        cases G <;> simp only [Frame.plug, reduceCtorEq, Tm.force.injEq, false_and] at ha
        exact (plug_value hr .loc ha.1).elim
    · rcases snoc_or E₀ with rfl | ⟨E₁, G, rfl⟩
      · simp only [plug] at ha; rw [ha.1]; trivial
      · rw [plug_snoc] at ha
        cases G <;> simp only [Frame.plug, reduceCtorEq, Tm.settle.injEq, false_and] at ha
        exact (plug_value hr (val hl) ha.1.2).elim
    · exact (plug_value hr (val hl) ha.1).elim
    · rw [plug_noframe nt ha.1]; trivial

/-- The left side of a special pair steps at its redex `r` in `F`. -/
theorem step_in {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' : List Tm} {E₁ : List Frame}
    {F : Frame} {r t' : Tm} (hE₁ : OkE E₁) (hF : F.ok) (hr : Redex r) (hs : r.Simple)
    (h : Step root l Λ μ (plug E₁ (F.plug r)) (.cfg Λ' μ' t')) :
    (∃ r', PHead r r' ∧ Λ' = Λ ∧ μ' = μ ∧ t' = plug E₁ (F.plug r')) ∨
      (∃ r', RHead Λ μ r Λ' μ' r' ∧ t' = plug E₁ (F.plug r')) :=
  Step.simple (E := F :: E₁) (hE₁.cons' hF) hr hs h

theorem born_closed {v : Tm} (h : (Tm.let_ (.lazy v) (.let_ (.force (.var 0)) (.var 1))).Closed) :
    v.Closed := fun i hi => h i (by simp only [Tm.Free]; exact Or.inl hi)

theorem lazy_closed {v : Tm} (h : (Tm.lazy v).Closed) : v.Closed := fun i hi => h i hi

theorem append_set_last {μ : List Tm} {c c' : Tm} : (μ ++ [c]).set μ.length c' = μ ++ [c'] := by
  simp

end Flick.Equiv
