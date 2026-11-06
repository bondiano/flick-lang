import Formal.Elab.CoreCg

/-!
# λFlick⁺ as a `Target`: the operations of the run on related terms

Related values give related results of the operations the steps use: a field (`Cg.field`),
an update (`Cg.setField`), a match (`Cg.matchP`), an environment (`Cg.envLookup`,
`Cg.envAppend`) and a layer (`Cg.build`). `ORel R` relates two options of related values.

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

theorem Cg.envLookup (hB : BNoVal B) {θ θ₂ : Flick.Tm} (h : Cg B false θ θ₂) (hv : Value θ)
    (S : Name) : ORel (Cg B false) (θ.envLookup S) (θ₂.envLookup S) := by
  induction θ generalizing θ₂ with
  | econs S' v θ _ ihθ =>
    cases h with
    | econs h₁ h₂ =>
      cases hv with
      | econs _ hθ =>
        simp only [Flick.Tm.envLookup]
        split
        · exact h₁
        · exact ihθ h₂ hθ
    | base h => exact (hB _ _ h hv).elim
  | _ => cases h <;> first | exact (hB _ _ ‹_› hv).elim | simp [Flick.Tm.envLookup, ORel]

theorem Cg.isEnv (hB : BNoVal B) {θ θ₂ : Flick.Tm} {x : Bool} (h : Cg B x θ θ₂) (hθ : IsEnv θ) :
    IsEnv θ₂ := by
  induction hθ generalizing θ₂ x with
  | enil => cases h with
    | enil => exact .enil
    | base h => exact (hB _ _ h .enil).elim
  | econs hv hθ ih => cases h with
    | econs h₁ h₂ => exact .econs (h₁.value hB hv).1 (ih h₂)
    | base h => exact (hB _ _ h (.econs hv hθ.value)).elim

theorem Cg.envAppend (hB : BNoVal B) {θ θ' a a' : Flick.Tm} (h : Cg B false θ θ') (hθ : IsEnv θ)
    (ha : Cg B false a a') : Cg B false (θ.envAppend a) (θ'.envAppend a') := by
  induction hθ generalizing θ' with
  | enil => cases h with
    | enil => exact ha
    | base h => exact (hB _ _ h .enil).elim
  | econs hv hθ ih => cases h with
    | econs h₁ h₂ => exact .econs h₁ (ih h₂)
    | base h => exact (hB _ _ h (.econs hv hθ.value)).elim

theorem build_ctor {f e : Flick.Tm} {K : Name} (h : f.head = .ctor K) : (Flick.Tm.app f e).build = none := by
  cases f with
  | const => simp [Flick.Tm.head] at h
  | app g a =>
    cases g with
    | const c => simp [Flick.Tm.head] at h
    | _ => rfl
  | _ => rfl

theorem Cg.build (hB : BNoVal B) {v : Flick.Tm} (hv : Value v) :
    ∀ {v₂ : Flick.Tm}, Cg B false v v₂ → ORel (Cg B false) v.build v₂.build := by
  induction hv with
  | ctorApp hf hK he =>
    intro v₂ h
    obtain ⟨_, hK', _⟩ := h.value hB (.ctorApp hf hK he)
    cases h with
    | app h₁ h₂ =>
      have hK₂ := hK' _ (by simpa [Flick.Tm.head] using hK)
      simp only [Flick.Tm.head] at hK₂
      rw [build_ctor hK, build_ctor hK₂]; trivial
    | base h => exact (hB _ _ h (.ctorApp hf hK he)).elim
  | @constApp e c hc he _ =>
    intro v₂ h
    cases h with
    | app h₁ h₂ =>
      cases h₁ with
      | const => cases c <;> simp [Flick.Tm.build, ORel] <;> constructor <;>
          first | exact h₂ | exact .enil | exact .await h₂
      | base h => exact (hB _ _ h .const).elim
    | base h => exact (hB _ _ h (.constApp hc he)).elim
  | @constApp2 e₁ e₂ c hc hl he₁ he₂ ih₁ ih₂ =>
    intro v₂ h
    cases h with
    | app h₁ h₂ =>
      cases h₁ with
      | app h₃ h₄ =>
        cases h₃ with
        | const =>
          cases c <;> simp only [Flick.Tm.build] <;> try trivial
          exact (ih₁ h₄).bind fun a b hab => (ih₂ h₂).bind fun c d hcd => .join hab hcd
        | base h => exact (hB _ _ h .const).elim
      | base h => exact (hB _ _ h (.constApp (.inl hc) he₁)).elim
    | base h => exact (hB _ _ h (.constApp2 hc hl he₁ he₂)).elim
  | _ =>
    intro v₂ h
    cases h <;> first | exact (hB _ _ ‹_› (by constructor <;> assumption)).elim |
      simp [Flick.Tm.build, ORel]

/-- The left side of a base pair is never `raise`, nor unwinding to a handler. -/
def BNoRaise (B : Flick.Tm → Flick.Tm → Prop) : Prop :=
  (∀ a b, B (.raise a) b → False) ∧ ∀ p b, B (.unw p) b → False

theorem Cg.final (hB : BNoVal B) (hR : BNoRaise B) {w w₂ : Flick.Tm} (h : Cg B false w w₂)
    (hw : Final w) : Final w₂ := by
  rcases hw with hw | ⟨v, hv, rfl⟩ | ⟨p, rfl⟩
  · exact .inl (h.value hB hw).1
  · cases h with
    | raise h => exact .inr (.inl ⟨_, (h.value hB hv).1, rfl⟩)
    | base h => exact (hR.1 _ _ h).elim
  · cases h with
    | unw => exact .inr (.inr ⟨_, rfl⟩)
    | base h => exact (hR.2 _ _ h).elim

end Flick.Elab
