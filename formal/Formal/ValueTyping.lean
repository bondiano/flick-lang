import Formal.Shapes

/-!
# Typing views through value conversion

Concrete runtime values keep their region or store location. An invariant
stored type can have a different row expression in the target type.
-/

namespace Flick

variable {S : Sig}

theorem Typ.loc_ref {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ l a φ, e = .loc l → J = .ex (.ref a) φ →
      ∃ u, Ψ[l]? = some u ∧ Ty.Equiv u a := by
  induction h with
  | conv _ he ih =>
    intro l a φ hl hJ; cases hJ
    cases he.symm.form with
    | rows hr _ => cases hr
    | ref ha =>
      obtain ⟨u, hu, heu⟩ := ih l _ _ hl rfl
      exact ⟨u, hu, heu.trans ha.symm⟩
  | loc hl => intro l a φ hv hJ; cases hv; cases hJ; exact ⟨_, hl, .refl⟩
  | allI | errVal => intro l a φ _ hJ; cases hJ
  | _ => intro l a φ hv _; cases hv

theorem Typ.task_eff {Ψ Θ Γ v J} (h : Typ S Ψ Θ Γ v J) :
    ∀ e r ε t C φ, v = .task e → J = .ex (.eff r ε t C) φ →
      ∃ t₀ ψ, Typ S Ψ Θ Γ e (.ex t₀ ψ) ∧ ψ.le (Stage.use (.rt r ε) C) ∧ LeLive S t₀ t := by
  induction h with
  | conv _ he ih =>
    intro e r ε t C φ hv hJ; cases hJ
    cases he.symm.form with
    | rows hr _ => cases hr
    | eff ht =>
      obtain ⟨t₀, ψ, hb, hψ, hT⟩ := ih e _ _ _ _ _ hv rfl
      exact ⟨t₀, ψ, hb, hψ, hT.trans (.equiv ht.symm)⟩
  | task he hψ _ hT =>
    intro e r ε t C φ hv hJ; cases hv; cases hJ
    obtain ⟨_, _, _, _, hh, huse, ht, -⟩ := hT.eff_left
    cases hh
    exact ⟨_, _, he, Stage.le_trans hψ huse, ht⟩
  | allI | errVal => intro e r ε t C φ _ hJ; cases hJ
  | _ => intro e r ε t C φ hv _; cases hv

theorem Typ.own_owner {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ p s φ, e = .own p → J = .ex (.owner s) φ → s = .rg p := by
  induction h with
  | conv _ he ih =>
    intro p s φ hv hJ; cases hJ
    cases he.symm.form with
    | rows hr _ => cases hr
    | owner hs =>
      rw [ih p _ _ hv rfl] at hs
      exact hs.rg_right
  | own => intro p s φ hv hJ; cases hv; cases hJ; rfl
  | allI | errVal => intro p s φ _ hJ; cases hJ
  | _ => intro p s φ hv _; cases hv

theorem Typ.cap_hnd {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ p s φ, e = .cap p → J = .ex (.hnd s) φ → s = .rg p := by
  induction h with
  | conv _ he ih =>
    intro p s φ hv hJ; cases hJ
    cases he.symm.form with
    | rows hr _ => cases hr
    | hnd hs =>
      rw [ih p _ _ hv rfl] at hs
      exact hs.rg_right
  | cap => intro p s φ hv hJ; cases hv; cases hJ; rfl
  | allI | errVal => intro p s φ _ hJ; cases hJ
  | _ => intro p s φ hv _; cases hv

end Flick
