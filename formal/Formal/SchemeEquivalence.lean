import Formal.SchemeOps
import Formal.TypeEquivArgs

/-!
# Instances of schemes at congruent arguments

The facts of a scheme are obligations modulo type congruence. Instantiating
its outer parameters by congruent types preserves the checked obligations.
-/

namespace Flick

theorem Scheme.Holds.subst_equiv {s : Scheme} {Θ ts} {σ τ : Atom → Ty}
    (hστ : ∀ a, Ty.Equiv (σ a) (τ a)) (h : (s.substT σ).Holds Θ ts) :
    (s.substT τ).Holds Θ ts := by
  rintro C r ⟨C₀, r₀, h₀, rfl, rfl⟩
  have hc := Caps.substT_equiv (liftT_equiv hστ s.k) C₀
  have hr := h _ _ ⟨C₀, r₀, h₀, rfl, rfl⟩
  rw [hc] at hr
  exact hr.equiv ((r₀.substT_equiv (liftT_equiv hστ s.k)).substT (instT ts))

end Flick
