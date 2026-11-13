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
    (h : LeLive S (.arrow A φ₀ t₀ C₀) (.arrow a φ t C)) :
    LeLive S a A ∧ (Stage.use φ₀ C₀).le (Stage.use φ C) ∧ LeLive S t₀ t := by
  obtain ⟨_, _, _, _, he, ha, hφ, ht, _⟩ := h.arrow_left
  cases he; exact ⟨ha, hφ, ht⟩

/-- The constant `op` asks for the region of its capability in the live discipline. -/
theorem Const.ty_op_arrow (S : Sig) (ι : CArg) :
    ∃ r, Const.ty S .op ι = .arrow (.hnd ι.s) r (.base .unit) Caps.none := by
  exact ⟨_, rfl⟩
/-- The first argument of `op` is the capability of its region. -/
theorem LeLive.op_dom {S : Sig} {ι : CArg} {a ψ t C}
    (h : LeLive S (Const.ty S .op ι) (.arrow a ψ t C)) : LeLive S a (.hnd ι.s) := by
  obtain ⟨r, hr⟩ := Const.ty_op_arrow S ι
  rw [hr] at h
  exact (LeLive.arrow_inv h).1
/-- A subtype of a plain type, neither an arrow, a computation nor data, is that type. -/
theorem LeLive.eq_of {a b : Ty} (hb : b.isPlain = true) (h : LeLive S a b) : Ty.Equiv a b :=
  h.eq_plain_right hb

theorem LeLive.eq_ref {a t : Ty} (h : LeLive S a (.ref t)) :
    Ty.Equiv a (.ref t) :=
  LeLive.eq_of rfl h

theorem LeLive.eq_owner {a s : Ty} (h : LeLive S a (.owner s)) :
    Ty.Equiv a (.owner s) :=
  LeLive.eq_of rfl h

theorem LeLive.eq_hnd {a s : Ty} (h : LeLive S a (.hnd s)) :
    Ty.Equiv a (.hnd s) :=
  LeLive.eq_of rfl h

theorem LeLive.eq_base {a : Ty} {b : Base}
    (h : LeLive S a (.base b)) : a = .base b :=
  (LeLive.eq_of rfl h).base_right

/-- A type above a plain type, neither an arrow, a computation nor data, is that type. -/
theorem LeLive.eq_of_left {a b : Ty} (ha : a.isPlain = true) (h : LeLive S a b) : Ty.Equiv a b :=
  (h.eq_plain ha).symm

theorem LeLive.eq_base_left {a : Ty} {b : Base}
    (h : LeLive S (.base b) a) : a = .base b :=
  (LeLive.eq_of_left (a := .base b) rfl h).symm.base_right

theorem LeLive.eq_ref_left {a t : Ty}
    (h : LeLive S (.ref t) a) : Ty.Equiv a (.ref t) :=
  (LeLive.eq_of_left (a := .ref t) rfl h).symm

theorem LeLive.eq_hnd_left {a s : Ty}
    (h : LeLive S (.hnd s) a) : Ty.Equiv a (.hnd s) :=
  (LeLive.eq_of_left (a := .hnd s) rfl h).symm

theorem LeLive.eq_owner_left {a s : Ty}
    (h : LeLive S (.owner s) a) : Ty.Equiv a (.owner s) :=
  (LeLive.eq_of_left (a := .owner s) rfl h).symm

theorem LeLive.typ_ref {Ψ Θ Γ e x t φ}
    (h : LeLive S x (.ref t)) (he : Typ S Ψ Θ Γ e (.ex x φ)) :
    Typ S Ψ Θ Γ e (.ex (.ref t) φ) := by
  exact .conv he (LeLive.eq_ref h)

theorem LeLive.typ_owner {Ψ Θ Γ e x s φ}
    (h : LeLive S x (.owner s)) (he : Typ S Ψ Θ Γ e (.ex x φ)) :
    Typ S Ψ Θ Γ e (.ex (.owner s) φ) := by
  exact .conv he (LeLive.eq_owner h)

theorem LeLive.typ_hnd {Ψ Θ Γ e x s φ}
    (h : LeLive S x (.hnd s)) (he : Typ S Ψ Θ Γ e (.ex x φ)) :
    Typ S Ψ Θ Γ e (.ex (.hnd s) φ) := by
  exact .conv he (LeLive.eq_hnd h)

theorem LeLive.typ_base {Ψ Θ Γ e x b φ}
    (h : LeLive S x (.base b)) (he : Typ S Ψ Θ Γ e (.ex x φ)) :
    Typ S Ψ Θ Γ e (.ex (.base b) φ) := by
  cases LeLive.eq_base h; exact he
end Flick
