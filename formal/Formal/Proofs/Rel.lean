import Formal.Proofs.Typing

/-!
# The logical equality of λPT

`R G t v w`: the values `v` and `w` are equal at the type `t`. Functions are
equal when they take equal arguments to equal results, which is funext.
`E` lifts it to terms: both evaluate, to equal values. A type variable
relates nothing, so a statement holds at a type with variables only where
it says nothing; it is read at every closed instance.

`R` is a partial equivalence: symmetric and transitive. A value is a value
of its type when it is related to itself.
-/

namespace Flick.PT

def R (G : List Tm) : Ty → Tm → Tm → Prop
  | .unit, .unit, .unit => True
  | .prod a b, .pair v₁ w₁, .pair v₂ w₂ => R G a v₁ v₂ ∧ R G b w₁ w₂
  | .sum a _, .inl v _, .inl w _ => R G a v w
  | .sum _ b, .inr _ v, .inr _ w => R G b v w
  | .arrow a b, .lam _ b₁, .lam _ b₂ => ∀ v w, R G a v w →
      ∃ v' w', Eval G (b₁.inst v) v' ∧ Eval G (b₂.inst w) w' ∧ R G b v' w'
  | _, _, _ => False

/-- Both terms evaluate, to equal values at `t`. -/
def E (G : List Tm) (t : Ty) (x y : Tm) : Prop :=
  ∃ v w, Eval G x v ∧ Eval G y w ∧ R G t v w

variable {G : List Tm}

theorem R.value {t : Ty} : ∀ {v w : Tm}, R G t v w → Value v ∧ Value w := by
  induction t with
  | var => intro v w h; cases v <;> cases w <;> simp only [R] at h
  | unit => intro v w h; cases v <;> cases w <;> simp only [R] at h; exact ⟨.unit, .unit⟩
  | prod a b iha ihb =>
    intro v w h; cases v <;> cases w <;> simp only [R] at h
    exact ⟨.pair (iha h.1).1 (ihb h.2).1, .pair (iha h.1).2 (ihb h.2).2⟩
  | sum a b iha ihb =>
    intro v w h; cases v <;> cases w <;> simp only [R] at h
    · exact ⟨.inl (iha h).1, .inl (iha h).2⟩
    · exact ⟨.inr (ihb h).1, .inr (ihb h).2⟩
  | arrow a b _ _ => intro v w h; cases v <;> cases w <;> simp only [R] at h; exact ⟨.lam, .lam⟩

theorem R.symm {t : Ty} : ∀ {v w : Tm}, R G t v w → R G t w v := by
  induction t with
  | var => intro v w h; cases v <;> cases w <;> simp only [R] at h
  | unit => intro v w h; cases v <;> cases w <;> simp only [R] at h ⊢
  | prod a b iha ihb =>
    intro v w h; cases v <;> cases w <;> simp only [R] at h ⊢; exact ⟨iha h.1, ihb h.2⟩
  | sum a b iha ihb =>
    intro v w h; cases v <;> cases w <;> simp only [R] at h ⊢
    · exact iha h
    · exact ihb h
  | arrow a b iha ihb =>
    intro v w h; cases v <;> cases w <;> simp only [R] at h ⊢
    intro x y hxy
    obtain ⟨v', w', hv, hw, hr⟩ := h y x (iha hxy)
    exact ⟨w', v', hw, hv, ihb hr⟩

theorem R.trans {t : Ty} : ∀ {u v w : Tm}, R G t u v → R G t v w → R G t u w := by
  induction t with
  | var => intro u v w h; cases u <;> cases v <;> simp only [R] at h
  | unit =>
    intro u v w h k
    cases u <;> cases v <;> simp only [R] at h <;> cases w <;> simp only [R] at k ⊢
  | prod a b iha ihb =>
    intro u v w h k
    cases u <;> cases v <;> simp only [R] at h <;> cases w <;> simp only [R] at k ⊢
    exact ⟨iha h.1 k.1, ihb h.2 k.2⟩
  | sum a b iha ihb =>
    intro u v w h k
    cases u <;> cases v <;> simp only [R] at h <;> cases w <;> simp only [R] at k ⊢
    · exact iha h k
    · exact ihb h k
  | arrow a b iha ihb =>
    intro u v w h k
    cases u <;> cases v <;> simp only [R] at h <;> cases w <;> simp only [R] at k ⊢
    intro x y hxy
    -- `y` is related to itself, so the middle function takes it to one value
    have hyy := iha (R.symm hxy) hxy
    obtain ⟨u', v', hu, hv, huv⟩ := h x y hxy
    obtain ⟨v'', w', hv', hw, hvw⟩ := k y y hyy
    cases hv.det hv'
    exact ⟨u', w', hu, hw, ihb huv hvw⟩

theorem E.symm {t : Ty} {x y : Tm} : E G t x y → E G t y x
  | ⟨v, w, hx, hy, h⟩ => ⟨w, v, hy, hx, h.symm⟩

theorem E.trans {t : Ty} {x y z : Tm} : E G t x y → E G t y z → E G t x z
  | ⟨u, _, hx, hy, h⟩, ⟨_, w, hy', hz, k⟩ => ⟨u, w, hx, hz, h.trans (hy.det hy' ▸ k)⟩

/-- Related values are related terms. -/
theorem R.E {t : Ty} {v w : Tm} (h : R G t v w) : E G t v w :=
  ⟨v, w, h.value.1.eval, h.value.2.eval, h⟩

/-- A term related to anything evaluates to a value related to itself. -/
theorem E.left {t : Ty} {x y : Tm} : E G t x y → E G t x x
  | h => h.trans h.symm

/-! ## Open terms -/

/-- `γ₁` and `γ₂` give related terms to the variables of `Γ` at `θ`: terms
that evaluate to related values. A substitution by a term that is not yet
a value is one, as `β` and `let` make. -/
def RS (G : List Tm) (θ : Nat → Ty) (Γ : List Ty) (γ₁ γ₂ : Nat → Tm) : Prop :=
  ∀ n t, Γ[n]? = some t → E G (t.subst θ) (γ₁ n) (γ₂ n)

theorem RS.cons {θ : Nat → Ty} {Γ : List Ty} {γ₁ γ₂ : Nat → Tm} {a : Ty} {v w : Tm}
    (hvw : E G (a.subst θ) v w) (h : RS G θ Γ γ₁ γ₂) :
    RS G θ (a :: Γ) (scons v γ₁) (scons w γ₂) := by
  intro n t hn
  cases n with
  | zero => simp at hn; subst hn; exact hvw
  | succ n => exact h n t (by simpa using hn)

theorem RS.symm {θ : Nat → Ty} {Γ : List Ty} {γ₁ γ₂ : Nat → Tm}
    (h : RS G θ Γ γ₁ γ₂) : RS G θ Γ γ₂ γ₁ :=
  fun n t hn => (h n t hn).symm

theorem RS.left {θ : Nat → Ty} {Γ : List Ty} {γ₁ γ₂ : Nat → Tm}
    (h : RS G θ Γ γ₁ γ₂) : RS G θ Γ γ₁ γ₁ :=
  fun n t hn => (h n t hn).left

/-- `e₁` and `e₂` are equal at `t` in `Γ`: under every instance of the type
variables and every two related substitutions for its variables. -/
def Equiv (G : List Tm) (Γ : List Ty) (e₁ e₂ : Tm) (t : Ty) : Prop :=
  ∀ θ γ₁ γ₂, RS G θ Γ γ₁ γ₂ → E G (t.subst θ) (e₁.subst γ₁) (e₂.subst γ₂)

theorem Equiv.symm {Γ : List Ty} {e₁ e₂ : Tm} {t : Ty} (h : Equiv G Γ e₁ e₂ t) :
    Equiv G Γ e₂ e₁ t :=
  fun θ γ₁ γ₂ hγ => (h θ γ₂ γ₁ hγ.symm).symm

theorem Equiv.trans {Γ : List Ty} {e₁ e₂ e₃ : Tm} {t : Ty} (h : Equiv G Γ e₁ e₂ t)
    (k : Equiv G Γ e₂ e₃ t) : Equiv G Γ e₁ e₃ t :=
  fun θ γ₁ γ₂ hγ => (h θ γ₁ γ₁ hγ.left).trans (k θ γ₁ γ₂ hγ)

end Flick.PT
