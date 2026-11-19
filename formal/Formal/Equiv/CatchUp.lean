import Formal.Equiv.Store
import Formal.Equiv.PotTm

/-!
# Contextual equivalence: the right side catches up

A rewrite the other way round (`dropC`, `forceC`) and the frame of a right cell (`jR`) put work
on the right side that the left one does not do: allocate a cell only the right side has, bind
it, force it, settle it. These steps touch only cells only the right side has (`RJ`), so they
keep related stores. Before a step of the left side uses a value, a raised error or a signal,
the right side reaches the same form by them (`Sim.vcu`, `Sim.rcu`, `Sim.ucu`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

/-- A head step in an ordered context that reads no region. -/
def AStep (μ : List Tm) (t : Tm) (μ' : List Tm) (t' : Tm) : Prop :=
  ∃ E e e', OkE E ∧ t = plug E e ∧ t' = plug E e' ∧
    ((PHead e e' ∧ μ' = μ) ∨ ∀ Λ, RHead Λ μ e Λ μ' e')

inductive AStar : List Tm → Tm → List Tm → Tm → Prop
  | refl {μ t} : AStar μ t μ t
  | step {μ t μ₁ t₁ μ' t'} : AStep μ t μ₁ t₁ → AStar μ₁ t₁ μ' t' → AStar μ t μ' t'

theorem AStep.plug {μ μ' : List Tm} {t t' : Tm} (h : AStep μ t μ' t')
    {E₀ : List Frame} (hE : OkE E₀) : AStep μ (Flick.plug E₀ t) μ' (Flick.plug E₀ t') := by
  obtain ⟨E, e, e', hE', rfl, rfl, hh⟩ := h
  exact ⟨E ++ E₀, e, e', hE'.append' hE, by rw [plug_append], by rw [plug_append], hh⟩

theorem AStar.plug {μ μ' : List Tm} {t t' : Tm} (h : AStar μ t μ' t')
    {E₀ : List Frame} (hE : OkE E₀) : AStar μ (Flick.plug E₀ t) μ' (Flick.plug E₀ t') := by
  induction h with
  | refl => exact .refl
  | step s _ ih => exact .step (s.plug hE) ih

theorem AStar.trans {μ μ₁ μ' : List Tm} {t t₁ t' : Tm} (h₁ : AStar μ t μ₁ t₁)
    (h₂ : AStar μ₁ t₁ μ' t') : AStar μ t μ' t' := by
  induction h₁ with
  | refl => exact h₂
  | step s _ ih => exact .step s (ih h₂)

theorem AStep.step {root : Roots} {μ μ' : List Tm} {t t' : Tm}
    (h : AStep μ t μ' t') (Λ : List Rgn) : ∃ l, Step root l Λ μ t (.cfg Λ μ' t') := by
  obtain ⟨E, e, e', hE, rfl, rfl, ⟨hh, rfl⟩ | hh⟩ := h
  · exact ⟨_, .pure hE hh⟩
  · exact ⟨_, .rt hE (hh Λ)⟩

theorem AStar.steps {root : Roots} {Λ : List Rgn} {μ μ' : List Tm} {t t' : Tm}
    (h : AStar μ t μ' t') : Steps root (fun _ => True) Λ μ t Λ μ' t' := by
  induction h with
  | refl => exact .refl
  | step s _ ih => obtain ⟨l, s'⟩ := s.step (root := root) Λ; exact .step s' trivial ih

/-- Steps of the right side that change only cells it alone has. -/
structure RJ (W : World) (μ : List Tm) (b : Tm) (W' : World) (μ' : List Tm) (b' : Tm) : Prop where
  steps : AStar μ b μ' b'
  le : W.le W'
  st : ∀ μ₁, StR W μ₁ μ → StR W' μ₁ μ'
  /-- The left cells, and the ones reserved for a force, stay. -/
  j₁ : ∀ n, W'.J₁ n → W.J₁ n
  o : ∀ n τ, W'.O n τ → W.O n τ
  /-- They consume forms only one side has: none, or the potential drops. -/
  pot : (μ' = μ ∧ b' = b) ∨ b'.pot < b.pot

theorem RJ.refl (μ : List Tm) (b : Tm) : RJ W μ b W μ b :=
  ⟨.refl, World.le.refl _, fun _ h => h, fun _ h => h, fun _ _ h => h, .inl ⟨rfl, rfl⟩⟩

theorem RJ.trans {W₁ W₂ : World} {μ μ₁ μ₂ : List Tm} {b b₁ b₂ : Tm} (h₁ : RJ W μ b W₁ μ₁ b₁)
    (h₂ : RJ W₁ μ₁ b₁ W₂ μ₂ b₂) : RJ W μ b W₂ μ₂ b₂ :=
  ⟨h₁.steps.trans h₂.steps, h₁.le.trans h₂.le, fun _ h => h₂.st _ (h₁.st _ h),
    fun _ h => h₁.j₁ _ (h₂.j₁ _ h), fun _ _ h => h₁.o _ _ (h₂.o _ _ h), by
      rcases h₁.pot with ⟨rfl, rfl⟩ | p₁ <;> rcases h₂.pot with ⟨rfl, rfl⟩ | p₂
      · exact .inl ⟨rfl, rfl⟩
      · exact .inr p₂
      · exact .inr p₁
      · exact .inr (Nat.lt_trans p₂ p₁)⟩

theorem RJ.plug {W' : World} {μ μ' : List Tm} {b b' : Tm} (h : RJ W μ b W' μ' b')
    {E : List Frame} (hE : OkE E) : RJ W μ (Flick.plug E b) W' μ' (Flick.plug E b') :=
  ⟨h.steps.plug hE, h.le, h.st, h.j₁, h.o, by
    rcases h.pot with ⟨rfl, rfl⟩ | h
    · exact .inl ⟨rfl, rfl⟩
    · exact .inr (pot_plug_lt E h)⟩

theorem RJ.frame {W' : World} {μ μ' : List Tm} {b b' : Tm} (h : RJ W μ b W' μ' b')
    {F : Frame} (hF : F.ok) : RJ W μ (F.plug b) W' μ' (F.plug b') :=
  h.plug (E := [F]) (OkE.cons' OkE.nil hF)

theorem RJ.head {μ μ' : List Tm} {b b' : Tm} (h : ∀ Λ, RHead Λ μ b Λ μ' b')
    (hs : ∀ μ₁, StR W μ₁ μ → StR W μ₁ μ') (hp : b'.pot < b.pot) : RJ W μ b W μ' b' :=
  ⟨.step ⟨[], b, b', OkE.nil, rfl, rfl, .inr h⟩ .refl, World.le.refl _, hs, fun _ h => h,
    fun _ _ h => h, .inr hp⟩

theorem RJ.pure {μ : List Tm} {b b' : Tm} (h : PHead b b') (hp : b'.pot < b.pot) : RJ W μ b W μ b' :=
  ⟨.step ⟨[], b, b', OkE.nil, rfl, rfl, .inl ⟨h, rfl⟩⟩ .refl, World.le.refl _,
    fun _ h => h, fun _ h => h, fun _ _ h => h, .inr hp⟩

/-- A new right cell only the right side has. -/
theorem RJ.alloc (μ : List Tm) (e : Tm) :
    RJ W μ (.lazy e) (W.addJ₂ μ.length) (μ ++ [.lazy e]) (.loc μ.length) :=
  ⟨.step ⟨[], _, _, OkE.nil, rfl, rfl, .inr fun _ => RHead.lazy⟩ .refl, World.le_addJ₂ _ _,
    fun _ h => h.allocJ₂ _, fun _ h => h, fun _ _ h => h, .inr (by simp [Tm.pot])⟩

theorem RJ.settleV {μ : List Tm} {n : Nat} {v : Tm} (hn : W.J₂ n) (hv : Value v) :
    RJ W μ (.settle n v) W (settleCell μ n v) v :=
  RJ.head (fun _ => .settleVal hv) (fun _ h => h.settleJ₂ hn _) (by simp [Tm.pot])

theorem RJ.settleR {μ : List Tm} {n : Nat} {v : Tm} (hn : W.J₂ n) (hv : Value v) :
    RJ W μ (.settle n (.raise v)) W (settleCell μ n (.raise v)) (.raise v) :=
  RJ.head (fun _ => .settleRaise hv) (fun _ h => h.settleJ₂ hn _) (by simp [Tm.pot])

theorem RJ.settleU {μ : List Tm} {n p ℓ : Nat} {a : Tm} (hn : W.J₂ n) :
    RJ W μ (.settle n (.unw p ℓ a)) W (resetCell μ n) (.unw p ℓ a) :=
  RJ.head (fun _ => .settleUnw) (fun _ h => h.resetJ₂ hn) (by simp [Tm.pot])

theorem subst_shift (c v : Tm) : (c.rename (· + 1)).subst (inst [v]) = c := by
  rw [Tm.subst_rename]
  exact Tm.subst_fix _ _ fun i _ => by simp [Function.comp, inst]

/-- The right side of O1a the other way round binds a new cell of its own, unused. -/
theorem RJ.drop (μ : List Tm) (e c' : Tm) :
    RJ W μ (.let_ (.lazy e) (c'.rename (· + 1))) (W.addJ₂ μ.length) (μ ++ [.lazy e]) c' := by
  have h₁ := (RJ.alloc (W := W) μ e).frame (F := .let_ (c'.rename (· + 1))) trivial
  simp only [Frame.plug] at h₁
  refine h₁.trans ?_
  have h₂ := RJ.pure (W := W.addJ₂ μ.length) (μ := μ ++ [.lazy e])
    (PHead.let_ (v := .loc μ.length) (b := c'.rename (· + 1)) .loc)
    (by rw [subst_shift]; simp [Tm.pot, Tm.pot_rename])
  rw [subst_shift] at h₂
  exact h₂

/-- The right side of O1c the other way round allocates a cell of its own and starts it. -/
theorem RJ.start (μ : List Tm) (e' : Tm) :
    RJ W μ (.force (.lazy e')) (W.addJ₂ μ.length) ((μ ++ [Tm.lazy e']).set μ.length (Tm.running e'))
      (.settle μ.length e') := by
  have hc : (μ ++ [Tm.lazy e'])[μ.length]? = some (.lazy e') := by simp
  exact ⟨.step ⟨[.force], _, _, OkE.nil.cons' trivial, rfl, rfl, .inr fun _ => RHead.lazy⟩
      (.step ⟨[], _, _, OkE.nil, rfl, rfl, .inr fun _ => .forcePending hc⟩ .refl),
    World.le_addJ₂ _ _, fun _ h => (h.allocJ₂ _).setJ₂ (Or.inr rfl) _, fun _ h => h,
    fun _ _ h => h, .inr (by simp [Tm.pot])⟩

end Flick.Equiv
