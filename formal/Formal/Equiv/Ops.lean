import Formal.Equiv.Dec
import Formal.StepSites

/-!
# Contextual equivalence: the operations of a step on related values

Values related at `top` have related fields (`Sim.field`), updates (`Sim.setField`) and matches
(`Sim.matchP`); the values a match binds substitute as related substitutions (`SubR.inst`).
-/

namespace Flick.Equiv

open Flick.Elab (All2 ORel)

variable {W : World}

theorem SubR.inst {vs ws : List Tm}
    (h : All2 (fun a b => (∀ y, y ≠ .park → Sim W y a b) ∧ b.NoIns) vs ws) :
    SubR W (Flick.inst vs) (Flick.inst ws) := by
  intro i
  unfold Flick.inst
  have hl := h.length
  by_cases hi : i < vs.length
  · rw [dif_pos hi, dif_pos (hl ▸ hi)]; exact h.get i hi (hl ▸ hi)
  · rw [dif_neg hi, dif_neg (hl ▸ hi), hl]; exact ⟨fun _ hy => .var hy, by simp [Tm.NoIns, Tm.Has]⟩

/-- A value as a substituted term. -/
theorem Sim.sub {x : Pos} {a b : Tm} (h : Sim W x a b) (ha : Value a) (hb : Value b) :
    (∀ y, y ≠ .park → Sim W y a b) ∧ b.NoIns := ⟨fun y _ => h.value_any ha hb y, h.value_noIns hb⟩

theorem All2.mem_right {α β : Type} {R : α → β → Prop} {P : β → Prop} :
    ∀ {l : List α} {l' : List β}, All2 R l l' → (∀ b ∈ l', P b) → All2 (fun a b => R a b ∧ P b) l l'
  | _, _, .nil, _ => .nil
  | _, _, .cons r h, hp => .cons ⟨r, hp _ (List.mem_cons_self ..)⟩
      (All2.mem_right h fun b hb => hp b (List.mem_cons_of_mem _ hb))

theorem Sim.field {x : Pos} {r r₂ : Tm} (h : Sim W x r r₂) (hx : x = .top) (l : Label) :
    ORel (Sim W .top) (r.field l) (r₂.field l) := by
  induction r generalizing r₂ x with
  | rcons l' e rest _ ihr =>
    cases h with
    | rcons _ h₁ h₂ =>
      subst hx
      simp only [Tm.field]
      split
      · exact h₁
      · exact ihr h₂ rfl
    | jR hx' => exact absurd hx hx'
    | _ => cases hx
  | drec c r ih =>
    cases h with
    | drec h _ => simp only [Tm.field]; exact ih h rfl
    | jR hx' => exact absurd hx hx'
    | _ => cases hx
  | _ =>
    cases h <;> first | (simp [Tm.field, ORel]; done) | (cases hx; done) | (subst hx; contradiction)

theorem Sim.setField {v v₂ w w₂ : Tm} (h : Sim W .top v v₂) (hw : Sim W .top w w₂) (l : Label) :
    ORel (Sim W .top) (v.setField l w) (v₂.setField l w₂) := by
  induction v generalizing v₂ with
  | rcons l' e r _ ihr =>
    generalize hx : Pos.top = x at h
    cases h with
    | rcons hp h₁ h₂ =>
      subst hx
      simp only [Tm.setField]
      split
      · exact .rcons hp hw h₂
      · have := ihr h₂
        revert this
        cases (_ : Tm).setField l w <;> cases (_ : Tm).setField l w₂ <;>
          simp [ORel] <;> exact fun h => .rcons hp h₁ h
    | jR hx' => exact absurd hx.symm hx'
    | _ => cases hx
  | _ =>
    generalize hx : Pos.top = x at h
    cases h <;> first | (simp [Tm.setField, ORel]; done) | (cases hx; done) |
      (subst hx; contradiction)

theorem ORel.app {α β : Type} {R : α → β → Prop} {o : Option (List α)} {o' : Option (List β)}
    (h : ORel (All2 R) o o') {f : List α → Option (List α)} {g : List β → Option (List β)}
    (hf : ∀ a b, All2 R a b → ORel (All2 R) (f a) (g b)) : ORel (All2 R) (o.bind f) (o'.bind g) :=
  Flick.Elab.ORel.bind h hf

theorem Sim.matchP {p : Pat} : ∀ {v v₂ : Tm}, Sim W .top v v₂ →
    ORel (All2 (Sim W .top)) (Flick.matchP p v) (Flick.matchP p v₂) := by
  induction p with
  | wild => intro v v₂ _; exact .nil
  | var => intro v v₂ h; exact .cons h .nil
  | lit l =>
    intro v v₂ h
    generalize hx : Pos.top = x at h
    cases h <;> (try (cases hx; done)) <;> (try (subst hx; contradiction)) <;> (try subst hx) <;>
      simp only [Flick.matchP] <;> (try split) <;> simp [ORel] <;> exact .nil
  | ctor K =>
    intro v v₂ h
    generalize hx : Pos.top = x at h
    cases h <;> (try (cases hx; done)) <;> (try (subst hx; contradiction)) <;> (try subst hx) <;>
      simp only [Flick.matchP] <;> (try split) <;> simp [ORel] <;> exact .nil
  | app p q ihp ihq =>
    intro v v₂ h
    generalize hx : Pos.top = x at h
    cases h <;> (try (cases hx; done)) <;> (try (subst hx; contradiction)) <;> (try subst hx) <;>
      simp only [Flick.matchP] <;> try trivial
    rename_i h₁ h₂
    exact Flick.Elab.ORel.bind (ihp h₁) fun a b hab =>
      Flick.Elab.ORel.bind (ihq h₂) fun c d hcd => Flick.Elab.All2.app hab hcd
  | rany =>
    intro v v₂ h
    generalize hx : Pos.top = x at h
    cases h <;> (try (cases hx; done)) <;> (try (subst hx; contradiction)) <;> (try subst hx) <;>
      simp [Flick.matchP, ORel] <;> exact .nil
  | rfield l p r ihp ihr =>
    intro v v₂ h
    simp only [Flick.matchP]
    exact Flick.Elab.ORel.bind (h.field rfl l) fun a b hab =>
      Flick.Elab.ORel.bind (ihp hab) fun c d hcd =>
        Flick.Elab.ORel.bind (ihr h) fun e f hef => Flick.Elab.All2.app hcd hef
  | or p q ihp ihq => intro v v₂ h; exact Flick.Elab.ORel.or (ihp h) (ihq h)

/-- Matched values as related substitutions. -/
theorem Sim.matchP_sub {p : Pat} {v v₂ : Tm} (h : Sim W .top v v₂) (hv₂ : Value v₂)
    {δ : List Tm} (hm : Flick.matchP p v = some δ) :
    ∃ δ₂, Flick.matchP p v₂ = some δ₂ ∧ SubR W (inst δ) (inst δ₂) := by
  obtain ⟨δ₂, h₂, hδ⟩ := (Sim.matchP (p := p) h).some_left hm
  have hn := h.value_noIns hv₂
  refine ⟨δ₂, h₂, SubR.inst (Flick.Elab.All2.mono (All2.mem_right hδ
    (P := fun b => b.NoIns) fun b hb hi => hn (matchP_has p h₂ b hb hi)) fun a b r => ?_)⟩
  refine ⟨fun y hy => ?_, r.2⟩
  cases y
  · exact r.1.lower
  · exact r.1
  · exact (hy rfl).elim

end Flick.Equiv
