import Formal.SchemeEquivalence

/-!
# Equivalent facts and schemes

Facts retain the same capture set and answer congruent type queries. Schemes
retain their number of binders and relate their facts and body by congruence.
-/

namespace Flick

def Facts.Equiv (Θ Φ : Facts) : Prop := ∀ C s, Θ.close C s ↔ Φ.close C s

theorem Facts.Equiv.refl (Θ : Facts) : Facts.Equiv Θ Θ := fun _ _ => Iff.rfl

theorem Facts.Equiv.symm {Θ Φ : Facts} (h : Facts.Equiv Θ Φ) : Facts.Equiv Φ Θ :=
  fun C s => (h C s).symm

theorem Facts.Equiv.trans {Θ Φ Ω : Facts} (h : Facts.Equiv Θ Φ) (h' : Facts.Equiv Φ Ω) :
    Facts.Equiv Θ Ω := fun C s => (h C s).trans (h' C s)

theorem Facts.Equiv.substT {Θ Φ : Facts} (h : Facts.Equiv Θ Φ) (σ : Atom → Ty) :
    Facts.Equiv (Θ.substT σ) (Φ.substT σ) := by
  intro C s
  constructor
  · rintro ⟨s₁, ⟨C₀, s₀, h₀, rfl, rfl⟩, he⟩
    obtain ⟨s₂, h₂, he₂⟩ := (h C₀ s₀).mp ⟨s₀, h₀, .refl⟩
    exact ⟨s₂.substT σ, ⟨C₀, s₂, h₂, rfl, rfl⟩, (he₂.substT σ).trans he⟩
  · rintro ⟨s₁, ⟨C₀, s₀, h₀, rfl, rfl⟩, he⟩
    obtain ⟨s₂, h₂, he₂⟩ := (h C₀ s₀).mpr ⟨s₀, h₀, .refl⟩
    exact ⟨s₂.substT σ, ⟨C₀, s₂, h₂, rfl, rfl⟩, (he₂.substT σ).trans he⟩

theorem Facts.Equiv.of_subst {σ τ : Atom → Ty} (h : ∀ a, Ty.Equiv (σ a) (τ a)) (Θ : Facts) :
    Facts.Equiv (Θ.substT σ) (Θ.substT τ) := by
  intro C s
  constructor
  · rintro ⟨s₁, ⟨C₀, s₀, h₀, rfl, rfl⟩, he⟩
    exact ⟨s₀.substT τ, ⟨C₀, s₀, h₀, Caps.substT_equiv h C₀, rfl⟩,
      (s₀.substT_equiv h).symm.trans he⟩
  · rintro ⟨s₁, ⟨C₀, s₀, h₀, rfl, rfl⟩, he⟩
    exact ⟨s₀.substT σ, ⟨C₀, s₀, h₀, (Caps.substT_equiv h C₀).symm, rfl⟩,
      (s₀.substT_equiv h).trans he⟩

structure Scheme.Equiv (a b : Scheme) : Prop where
  arity : a.k = b.k
  facts : Facts.Equiv a.cs b.cs
  body : Ty.Equiv a.t b.t

theorem Scheme.Equiv.refl (s : Scheme) : s.Equiv s := ⟨rfl, .refl _, .refl⟩

theorem Scheme.Equiv.symm {a b : Scheme} (h : a.Equiv b) : b.Equiv a :=
  ⟨h.arity.symm, h.facts.symm, h.body.symm⟩

theorem Scheme.Equiv.mono {a b : Ty} (h : Ty.Equiv a b) : (Scheme.mono a).Equiv (.mono b) :=
  ⟨rfl, .refl _, h⟩

theorem Scheme.Equiv.substT {a b : Scheme} (h : a.Equiv b) (σ : Atom → Ty) :
    (a.substT σ).Equiv (b.substT σ) := by
  rcases a with ⟨k, cs, t⟩
  rcases b with ⟨k', cs', t'⟩
  have hk := h.arity
  cases hk
  exact ⟨rfl, h.facts.substT _, h.body.substT _⟩

theorem Scheme.Equiv.of_subst {σ τ : Atom → Ty} (h : ∀ a, Ty.Equiv (σ a) (τ a)) (s : Scheme) :
    (s.substT σ).Equiv (s.substT τ) :=
  ⟨rfl, .of_subst (liftT_equiv h s.k) s.cs, s.t.substT_equiv (liftT_equiv h s.k)⟩

theorem Scheme.Equiv.inst {a b : Scheme} (h : a.Equiv b) (ts : List Ty) :
    Ty.Equiv (a.inst ts) (b.inst ts) := h.body.substT _

theorem Scheme.Equiv.all {a b : Scheme} (h : a.Equiv b) :
    Ty.Equiv (.all a.k a.t) (.all b.k b.t) := by
  rw [← h.arity]
  exact .all h.body

theorem Scheme.Equiv.holds {a b : Scheme} (h : a.Equiv b) {Θ ts} (ha : a.Holds Θ ts) :
    b.Holds Θ ts := by
  intro C s hs
  obtain ⟨s₀, h₀, he⟩ := (h.facts C s).mpr ⟨s, hs, .refl⟩
  exact (ha C s₀ h₀).equiv (he.substT (instT ts))

theorem Scheme.Equiv.no_facts {a b : Scheme} (h : a.Equiv b) (ha : ∀ C s, ¬ a.cs C s) :
    ∀ C s, ¬ b.cs C s := by
  intro C s hs
  obtain ⟨s₀, h₀, _⟩ := (h.facts C s).mpr ⟨s, hs, .refl⟩
  exact ha C s₀ h₀

end Flick
