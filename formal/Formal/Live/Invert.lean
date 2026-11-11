import Formal.Live.Order

/-!
# Inversion of subtyping by use

Both disciplines preserve type formers. The stage comparison is on effective
use stages, not on a qualified value's raw latent stage.
-/

namespace Flick

/-- Both disciplines relate two arrows componentwise: the target's argument is below
the source's, the source's use stage is below the target's, and so is its result. -/
theorem LeLive.arrow_inv {A φ₀ t₀ C₀ a φ t C}
    (h : LeLive (.arrow A φ₀ t₀ C₀) (.arrow a φ t C)) :
    LeLive a A ∧ (Stage.use φ₀ C₀).le (Stage.use φ C) ∧ LeLive t₀ t := by
  cases h with
  | refl => exact ⟨.refl, Stage.le_refl _, .refl⟩
  | arrow h₁ h₂ h₃ => exact ⟨h₁, h₂, h₃⟩

/-- The constant `op` asks for the region of its capability in the live discipline. -/
theorem Const.ty_op_arrow (S : Sig) (ι : CArg) :
    ∃ r, Const.ty S .op ι = .arrow (.hnd ι.s) r (.base .unit) Caps.none := by
  exact ⟨_, rfl⟩
/-- The first argument of `op` is the capability of its region. -/
theorem LeLive.op_dom {S : Sig} {ι : CArg} {a ψ t C}
    (h : LeLive (Const.ty S .op ι) (.arrow a ψ t C)) : LeLive a (.hnd ι.s) := by
  obtain ⟨r, hr⟩ := Const.ty_op_arrow S ι
  rw [hr] at h
  exact (LeLive.arrow_inv h).1
/-- A subtype of a type that is neither an arrow nor a computation is that type. -/
theorem LeLive.eq_of {a b : Ty}
    (ha : ∀ x φ y C, b ≠ .arrow x φ y C) (he : ∀ r ε y C, b ≠ .eff r ε y C)
    (h : LeLive a b) : a = b := by
  cases h with
  | refl => rfl
  | arrow => exact absurd rfl (ha _ _ _ _)
  | eff => exact absurd rfl (he _ _ _ _)

theorem LeLive.eq_ref {a t : Ty} (h : LeLive a (.ref t)) :
    a = .ref t :=
  LeLive.eq_of (fun _ _ _ _ h => by cases h) (fun _ _ _ _ h => by cases h) h

theorem LeLive.eq_owner {a s : Ty} (h : LeLive a (.owner s)) :
    a = .owner s :=
  LeLive.eq_of (fun _ _ _ _ h => by cases h) (fun _ _ _ _ h => by cases h) h

theorem LeLive.eq_hnd {a s : Ty} (h : LeLive a (.hnd s)) :
    a = .hnd s :=
  LeLive.eq_of (fun _ _ _ _ h => by cases h) (fun _ _ _ _ h => by cases h) h

theorem LeLive.eq_base {a : Ty} {b : Base}
    (h : LeLive a (.base b)) : a = .base b :=
  LeLive.eq_of (fun _ _ _ _ h => by cases h) (fun _ _ _ _ h => by cases h) h

/-- A type below one that is neither an arrow nor a computation is that type. -/
theorem LeLive.eq_of_left {a b : Ty}
    (ha : ∀ x φ y C, a ≠ .arrow x φ y C) (he : ∀ r ε y C, a ≠ .eff r ε y C)
    (h : LeLive a b) : a = b := by
  cases h with
  | refl => rfl
  | arrow => exact absurd rfl (ha _ _ _ _)
  | eff => exact absurd rfl (he _ _ _ _)

theorem LeLive.eq_base_left {a : Ty} {b : Base}
    (h : LeLive (.base b) a) : a = .base b :=
  ((LeLive.eq_of_left (a := .base b) (fun _ _ _ _ h => by cases h)
    (fun _ _ _ _ h => by cases h) h)).symm

theorem LeLive.eq_ref_left {a t : Ty}
    (h : LeLive (.ref t) a) : a = .ref t :=
  ((LeLive.eq_of_left (a := .ref t) (fun _ _ _ _ h => by cases h)
    (fun _ _ _ _ h => by cases h) h)).symm

theorem LeLive.eq_hnd_left {a s : Ty}
    (h : LeLive (.hnd s) a) : a = .hnd s :=
  ((LeLive.eq_of_left (a := .hnd s) (fun _ _ _ _ h => by cases h)
    (fun _ _ _ _ h => by cases h) h)).symm

theorem LeLive.eq_owner_left {a s : Ty}
    (h : LeLive (.owner s) a) : a = .owner s :=
  ((LeLive.eq_of_left (a := .owner s) (fun _ _ _ _ h => by cases h)
    (fun _ _ _ _ h => by cases h) h)).symm

theorem LeLive.typ_ref {Ψ Θ Γ e x t φ}
    (h : LeLive x (.ref t)) (he : Typ S Ψ Θ Γ e (.ex x φ)) :
    Typ S Ψ Θ Γ e (.ex (.ref t) φ) := by
  cases LeLive.eq_ref h; exact he

theorem LeLive.typ_owner {Ψ Θ Γ e x s φ}
    (h : LeLive x (.owner s)) (he : Typ S Ψ Θ Γ e (.ex x φ)) :
    Typ S Ψ Θ Γ e (.ex (.owner s) φ) := by
  cases LeLive.eq_owner h; exact he

theorem LeLive.typ_hnd {Ψ Θ Γ e x s φ}
    (h : LeLive x (.hnd s)) (he : Typ S Ψ Θ Γ e (.ex x φ)) :
    Typ S Ψ Θ Γ e (.ex (.hnd s) φ) := by
  cases LeLive.eq_hnd h; exact he

theorem LeLive.typ_base {Ψ Θ Γ e x b φ}
    (h : LeLive x (.base b)) (he : Typ S Ψ Θ Γ e (.ex x φ)) :
    Typ S Ψ Θ Γ e (.ex (.base b) φ) := by
  cases LeLive.eq_base h; exact he
end Flick
