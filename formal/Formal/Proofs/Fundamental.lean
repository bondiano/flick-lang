import Formal.Proofs.Rel

/-!
# The fundamental lemma of λPT

Every term former preserves `Equiv`, the equality of open terms. With
the globals of a well-formed program, a typed term is equal to itself:
under every instance of its type variables and every two related
substitutions, it evaluates, to related values. So every typed term of
λPT terminates (`terminates`), and equal parts make equal wholes.
-/

namespace Flick.PT

variable {G : List Tm}

/-- Each global evaluates, at every instance of its scheme, to a value
related to itself. -/
def GlobOK (G : List Tm) (GT : List Ty) : Prop :=
  ∀ (g : Nat) (l : List Ty) (σ : Ty) (θ : Nat → Ty), GT[g]? = some σ →
    E G (σ.subst θ) (.glob g l) (.glob g l)

theorem compat_var {Γ : List Ty} {n : Nat} {t : Ty} (h : Γ[n]? = some t) :
    Equiv G Γ (.var n) (.var n) t :=
  fun _ _ _ hγ => hγ n t h

theorem compat_glob {GT : List Ty} {Γ : List Ty} {g : Nat} {σ : Ty} (l : List Ty)
    (θ' : Nat → Ty) (hG : GlobOK G GT) (h : GT[g]? = some σ) :
    Equiv G Γ (.glob g l) (.glob g l) (σ.subst θ') := by
  intro θ _ _ _
  rw [Ty.subst_subst]
  exact hG g l σ _ h

theorem compat_lam {Γ : List Ty} {a t d₁ d₂ : Ty} {b₁ b₂ : Tm}
    (h : Equiv G (a :: Γ) b₁ b₂ t) : Equiv G Γ (.lam d₁ b₁) (.lam d₂ b₂) (.arrow a t) := by
  intro θ γ₁ γ₂ hγ
  refine ⟨_, _, .lam, .lam, ?_⟩
  simp only [Ty.subst, R]
  intro v w hvw
  rw [Tm.subst_up_inst, Tm.subst_up_inst]
  exact h θ _ _ (hγ.cons hvw.E)

/-- The function an evaluated term of an arrow type is. -/
theorem R.arrow_lam {a t : Ty} {f g : Tm} (h : R G (.arrow a t) f g) :
    ∃ a₁ b₁ a₂ b₂, f = .lam a₁ b₁ ∧ g = .lam a₂ b₂ ∧ ∀ v w, R G a v w →
      ∃ v' w', Eval G (b₁.inst v) v' ∧ Eval G (b₂.inst w) w' ∧ R G t v' w' := by
  cases f <;> cases g <;> simp only [R] at h
  exact ⟨_, _, _, _, rfl, rfl, h⟩

theorem compat_app {Γ : List Ty} {a t : Ty} {f₁ f₂ x₁ x₂ : Tm}
    (hf : Equiv G Γ f₁ f₂ (.arrow a t)) (hx : Equiv G Γ x₁ x₂ a) :
    Equiv G Γ (.app f₁ x₁) (.app f₂ x₂) t := by
  intro θ γ₁ γ₂ hγ
  obtain ⟨vf₁, vf₂, ef₁, ef₂, rf⟩ := hf θ γ₁ γ₂ hγ
  obtain ⟨vx₁, vx₂, ex₁, ex₂, rx⟩ := hx θ γ₁ γ₂ hγ
  simp only [Ty.subst] at rf
  obtain ⟨_, b₁, _, b₂, rfl, rfl, hb⟩ := R.arrow_lam rf
  obtain ⟨v, w, ev, ew, r⟩ := hb vx₁ vx₂ rx
  exact ⟨v, w, .app ef₁ ex₁ ev, .app ef₂ ex₂ ew, r⟩

theorem compat_unit {Γ : List Ty} : Equiv G Γ .unit .unit .unit :=
  fun _ _ _ _ => ⟨_, _, .unit, .unit, by simp only [Ty.subst, R]⟩

theorem compat_pair {Γ : List Ty} {a b : Ty} {x₁ x₂ y₁ y₂ : Tm}
    (hx : Equiv G Γ x₁ x₂ a) (hy : Equiv G Γ y₁ y₂ b) :
    Equiv G Γ (.pair x₁ y₁) (.pair x₂ y₂) (.prod a b) := by
  intro θ γ₁ γ₂ hγ
  obtain ⟨v₁, v₂, e₁, e₂, r⟩ := hx θ γ₁ γ₂ hγ
  obtain ⟨w₁, w₂, f₁, f₂, s⟩ := hy θ γ₁ γ₂ hγ
  exact ⟨_, _, .pair e₁ f₁, .pair e₂ f₂, by simp only [Ty.subst, R]; exact ⟨r, s⟩⟩

theorem compat_split {Γ : List Ty} {a b t : Ty} {e₁ e₂ c₁ c₂ : Tm}
    (he : Equiv G Γ e₁ e₂ (.prod a b)) (hc : Equiv G (b :: a :: Γ) c₁ c₂ t) :
    Equiv G Γ (.split e₁ c₁) (.split e₂ c₂) t := by
  intro θ γ₁ γ₂ hγ
  obtain ⟨p₁, p₂, ep₁, ep₂, rp⟩ := he θ γ₁ γ₂ hγ
  simp only [Ty.subst] at rp
  cases p₁ <;> cases p₂ <;> simp only [R] at rp
  obtain ⟨u, w, eu, ew, r⟩ := hc θ _ _ ((hγ.cons rp.1.E).cons rp.2.E)
  refine ⟨u, w, .split ep₁ ?_, .split ep₂ ?_, r⟩
  · rw [Tm.subst_up2_inst2]; exact eu
  · rw [Tm.subst_up2_inst2]; exact ew

theorem compat_inl {Γ : List Ty} {a b d₁ d₂ : Ty} {x₁ x₂ : Tm} (hx : Equiv G Γ x₁ x₂ a) :
    Equiv G Γ (.inl x₁ d₁) (.inl x₂ d₂) (.sum a b) := by
  intro θ γ₁ γ₂ hγ
  obtain ⟨v₁, v₂, e₁, e₂, r⟩ := hx θ γ₁ γ₂ hγ
  exact ⟨_, _, .inl e₁, .inl e₂, by simp only [Ty.subst, R]; exact r⟩

theorem compat_inr {Γ : List Ty} {a b d₁ d₂ : Ty} {x₁ x₂ : Tm} (hx : Equiv G Γ x₁ x₂ b) :
    Equiv G Γ (.inr d₁ x₁) (.inr d₂ x₂) (.sum a b) := by
  intro θ γ₁ γ₂ hγ
  obtain ⟨v₁, v₂, e₁, e₂, r⟩ := hx θ γ₁ γ₂ hγ
  exact ⟨_, _, .inr e₁, .inr e₂, by simp only [Ty.subst, R]; exact r⟩

theorem compat_case {Γ : List Ty} {a b t : Ty} {e₁ e₂ l₁ l₂ r₁ r₂ : Tm}
    (he : Equiv G Γ e₁ e₂ (.sum a b)) (hl : Equiv G (a :: Γ) l₁ l₂ t)
    (hr : Equiv G (b :: Γ) r₁ r₂ t) :
    Equiv G Γ (.case e₁ l₁ r₁) (.case e₂ l₂ r₂) t := by
  intro θ γ₁ γ₂ hγ
  obtain ⟨s₁, s₂, es₁, es₂, rs⟩ := he θ γ₁ γ₂ hγ
  simp only [Ty.subst] at rs
  cases s₁ <;> cases s₂ <;> simp only [R] at rs
  · obtain ⟨u, w, eu, ew, r⟩ := hl θ _ _ (hγ.cons rs.E)
    refine ⟨u, w, .caseL es₁ ?_, .caseL es₂ ?_, r⟩
    · rw [Tm.subst_up_inst]; exact eu
    · rw [Tm.subst_up_inst]; exact ew
  · obtain ⟨u, w, eu, ew, r⟩ := hr θ _ _ (hγ.cons rs.E)
    refine ⟨u, w, .caseR es₁ ?_, .caseR es₂ ?_, r⟩
    · rw [Tm.subst_up_inst]; exact eu
    · rw [Tm.subst_up_inst]; exact ew

theorem compat_let {Γ : List Ty} {a t : Ty} {e₁ e₂ c₁ c₂ : Tm}
    (he : Equiv G Γ e₁ e₂ a) (hc : Equiv G (a :: Γ) c₁ c₂ t) :
    Equiv G Γ (.let_ e₁ c₁) (.let_ e₂ c₂) t := by
  intro θ γ₁ γ₂ hγ
  obtain ⟨v₁, v₂, ev₁, ev₂, rv⟩ := he θ γ₁ γ₂ hγ
  obtain ⟨u, w, eu, ew, r⟩ := hc θ _ _ (hγ.cons rv.E)
  refine ⟨u, w, .let_ ev₁ ?_, .let_ ev₂ ?_, r⟩
  · rw [Tm.subst_up_inst]; exact eu
  · rw [Tm.subst_up_inst]; exact ew

/-- A global evaluates as its body, whatever instance it is written at. -/
theorem compat_glob₂ {GT : List Ty} {Γ : List Ty} {g : Nat} {σ : Ty} (l₁ l₂ : List Ty)
    (θ' : Nat → Ty) (hG : GlobOK G GT) (h : GT[g]? = some σ) :
    Equiv G Γ (.glob g l₁) (.glob g l₂) (σ.subst θ') := by
  intro θ _ _ _
  rw [Ty.subst_subst]
  obtain ⟨v, w, ev, ew, r⟩ := hG g l₁ σ _ h
  cases ew with
  | glob hb eb => exact ⟨v, w, ev, .glob hb eb, r⟩

/-- **The fundamental lemma**: a typed term is equal to itself. -/
theorem fundamental {GT : List Ty} {Γ : List Ty} {e : Tm} {t : Ty} (hG : GlobOK G GT)
    (h : Typed GT Γ e t) : Equiv G Γ e e t := by
  induction h with
  | var hn => exact compat_var hn
  | glob hg => exact compat_glob _ _ hG hg
  | lam _ ih => exact compat_lam ih
  | app _ _ ihf ihx => exact compat_app ihf ihx
  | unit => exact compat_unit
  | pair _ _ ihx ihy => exact compat_pair ihx ihy
  | split _ _ ihe ihc => exact compat_split ihe ihc
  | inl _ ih => exact compat_inl ih
  | inr _ ih => exact compat_inr ih
  | case _ _ _ ihe ihl ihr => exact compat_case ihe ihl ihr
  | let_ _ _ ihe ihc => exact compat_let ihe ihc

/-- The globals of a well-formed program are related to themselves. -/
theorem Prog.WF.globOK {P : Prog} (hP : P.WF) : GlobOK P.bodies P.schemes := by
  -- by induction on the globals: a body names only the globals before it
  suffices h : ∀ k, ∀ g < k, ∀ (l : List Ty) σ θ, P.schemes[g]? = some σ →
      E P.bodies (σ.subst θ) (.glob g l) (.glob g l) by
    intro g l σ θ hg
    exact h (g + 1) g (Nat.lt_succ_self g) l σ θ hg
  intro k
  induction k with
  | zero => intro g hg; exact absurd hg (Nat.not_lt_zero g)
  | succ k ih =>
    intro g hg l σ θ hσ
    rcases Nat.lt_succ_iff_lt_or_eq.mp hg with hlt | rfl
    · exact ih g hlt l σ θ hσ
    · have hlen : g < P.length := by
        have := (List.getElem?_eq_some_iff.mp hσ).1
        simpa [Prog.schemes] using this
      have hσ' : P[g].2 = σ := by
        have := List.getElem?_eq_some_iff.mp hσ
        simpa [Prog.schemes] using this.2
      have hGT : GlobOK P.bodies (P.schemes.take g) := by
        intro g' l' σ' θ' h'
        have hlt : g' < g := by
          have := (List.getElem?_eq_some_iff.mp h').1
          simp at this; omega
        rw [List.getElem?_take_of_lt hlt] at h'
        exact ih g' hlt l' σ' θ' h'
      have hb := fundamental hGT (hP g hlen) θ Tm.var Tm.var (fun n t h => by simp at h)
      rw [(hP g hlen).closed, hσ'] at hb
      obtain ⟨v, w, ev, ew, r⟩ := hb
      have hbody : P.bodies[g]? = some P[g].1 := by simp [Prog.bodies, hlen]
      exact ⟨v, w, .glob hbody ev, .glob hbody ew, r⟩

/-- Every typed term of a well-formed program terminates, under every
instance of its type variables and every values of its variables. -/
theorem terminates {P : Prog} (hP : P.WF) {Γ : List Ty} {e : Tm} {t : Ty}
    (h : Typed P.schemes Γ e t) (θ : Nat → Ty) (γ : Nat → Tm) (hγ : RS P.bodies θ Γ γ γ) :
    ∃ v, Eval P.bodies (e.subst γ) v ∧ R P.bodies (t.subst θ) v v :=
  let ⟨v, _, ev, ew, r⟩ := fundamental hP.globOK h θ γ γ hγ
  ⟨v, ev, ew.det ev ▸ (r.trans r.symm)⟩

end Flick.PT
