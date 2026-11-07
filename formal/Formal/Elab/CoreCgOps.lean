import Formal.Elab.CoreCg

/-!
# λFlick⁺ as a `Target`: the operations of the run on related terms

Related values give related results of the operations the steps use: a field (`Cg.field`),
an update (`Cg.setField`) and a match (`Cg.matchP`). `ORel R` relates two options of related values.

`FrR B F F'`: two frames of one kind with related parts. A related term around a hole is
a base pair at some frame or the same frames around a related hole (`Cg.decomp`).
-/

namespace Flick.Elab

/-- Both `none`, or both `some` of related values. -/
def ORel {α β : Type} (R : α → β → Prop) : Option α → Option β → Prop
  | none, none => True
  | some a, some b => R a b
  | _, _ => False

theorem ORel.bind {α β γ δ : Type} {R : α → β → Prop} {R' : γ → δ → Prop} {o : Option α}
    {o' : Option β} (h : ORel R o o') {f : α → Option γ} {g : β → Option δ}
    (hf : ∀ a b, R a b → ORel R' (f a) (g b)) : ORel R' (o.bind f) (o'.bind g) := by
  cases o <;> cases o' <;> simp_all [ORel]

theorem ORel.or {α β : Type} {R : α → β → Prop} {o o' : Option α} {p p' : Option β}
    (h : ORel R o p) (h' : ORel R o' p') : ORel R (o.or o') (p.or p') := by
  cases o <;> cases p <;> simp_all [ORel]

theorem ORel.some_left {α β : Type} {R : α → β → Prop} {o : Option α} {o' : Option β} {a : α}
    (h : ORel R o o') (ha : o = some a) : ∃ b, o' = some b ∧ R a b := by
  subst ha; cases o' <;> simp_all [ORel]

theorem ORel.none_left {α β : Type} {R : α → β → Prop} {o : Option α} {o' : Option β}
    (h : ORel R o o') (ha : o = none) : o' = none := by
  subst ha; cases o' <;> simp_all [ORel]

theorem All2.app {α β : Type} {R : α → β → Prop} {l₁ l₂ : List α} {m₁ m₂ : List β}
    (h₁ : All2 R l₁ m₁) (h₂ : All2 R l₂ m₂) : All2 R (l₁ ++ l₂) (m₁ ++ m₂) := by
  induction h₁ with
  | nil => exact h₂
  | cons r _ ih => exact .cons r ih

variable {B : Flick.Tm → Flick.Tm → Prop}

theorem Cg.field {r r₂ : Flick.Tm} (h : Cg B true r r₂) (l : Label) :
    ORel (Cg B true) (r.field l) (r₂.field l) := by
  induction r generalizing r₂ with
  | rcons l' e rest _ ihr =>
    cases h with
    | rcons h₁ h₂ =>
      simp only [Flick.Tm.field]
      split
      · exact h₁
      · exact ihr h₂
  | drec c r ih => cases h with
    | drec h => simp only [Flick.Tm.field]; exact ih h
  | _ => cases h <;> simp [Flick.Tm.field, ORel]

theorem Cg.matchP {p : Pat} : ∀ {v v₂ : Flick.Tm}, Cg B true v v₂ →
    ORel (All2 (Cg B true)) (Flick.matchP p v) (Flick.matchP p v₂) := by
  induction p with
  | wild => intro v v₂ _; exact .nil
  | var => intro v v₂ h; exact .cons h .nil
  | lit l => intro v v₂ h; cases h <;> simp only [Flick.matchP] <;> (try split) <;> simp [ORel] <;>
      exact .nil
  | ctor K => intro v v₂ h; cases h <;> simp only [Flick.matchP] <;> (try split) <;> simp [ORel] <;>
      exact .nil
  | app p q ihp ihq =>
    intro v v₂ h
    cases h <;> simp only [Flick.matchP] <;> try trivial
    rename_i h₁ h₂
    exact (ihp h₁).bind fun a b hab => (ihq h₂).bind fun c d hcd => hab.app hcd
  | rany => intro v v₂ h; cases h <;> simp [Flick.matchP, ORel] <;> exact .nil
  | rfield l p r ihp ihr =>
    intro v v₂ h
    simp only [Flick.matchP]
    exact (h.field l).bind fun a b hab => (ihp hab).bind fun c d hcd =>
      (ihr h).bind fun e f hef => hcd.app hef
  | or p q ihp ihq => intro v v₂ h; exact (ihp h).or (ihq h)

/-- The left side of a base pair is never `raise`, nor unwinding to a handler. -/
def BNoRaise (B : Flick.Tm → Flick.Tm → Prop) : Prop :=
  (∀ a b, B (.raise a) b → False) ∧ ∀ p ℓ a b, B (.unw p ℓ a) b → False

theorem Cg.final (hB : BNoVal B) (hR : BNoRaise B) {w w₂ : Flick.Tm} (h : Cg B false w w₂)
    (hw : Final w) : Final w₂ := by
  rcases hw with hw | ⟨v, hv, rfl⟩ | ⟨p, ℓ, a, rfl⟩
  · exact .inl (h.value hB hw).1
  · cases h with
    | raise h => exact .inr (.inl ⟨_, (h.value hB hv).1, rfl⟩)
    | base h => exact (hR.1 _ _ h).elim
  · cases h with
    | unw => exact .inr (.inr ⟨_, _, _, rfl⟩)
    | base h => exact (hR.2 _ _ _ _ h).elim

end Flick.Elab
