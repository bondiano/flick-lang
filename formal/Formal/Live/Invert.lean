import Formal.Live.Order

/-!
# Inversion of mode-indexed subtyping

Both disciplines preserve type formers. The stage comparison is on effective
use stages, not on a qualified value's raw latent stage.
-/

namespace Flick

theorem TypingMode.subtype.arrow_left {mode : TypingMode} {a φ t C T}
    (h : mode.subtype (.arrow a φ t C) T) :
    ∃ a' φ' t' C', T = .arrow a' φ' t' C' ∧ mode.subtype a' a ∧
      (mode.use φ C).le (mode.use φ' C') ∧ mode.subtype t t' ∧
      Caps.Sub (Ty.arrow a φ t C).reach T.reach := by
  cases mode with
  | captures => exact Le.arrow_left h
  | live => exact LeLive.arrow_left h

theorem TypingMode.subtype.arrow_right {mode : TypingMode} {a φ t C T}
    (h : mode.subtype T (.arrow a φ t C)) :
    ∃ a' φ' t' C', T = .arrow a' φ' t' C' ∧ mode.subtype a a' ∧
      (mode.use φ' C').le (mode.use φ C) ∧ mode.subtype t' t ∧
      Caps.Sub T.reach (Ty.arrow a φ t C).reach := by
  cases mode with
  | captures => exact Le.arrow_right h
  | live => exact LeLive.arrow_right h

theorem TypingMode.subtype.eff_left_mode {mode : TypingMode} {r ε t C T}
    (h : mode.subtype (.eff r ε t C) T) :
    ∃ r' ε' t' C', T = .eff r' ε' t' C' ∧
      (mode.use (.rt r ε) C).le (mode.use (.rt r' ε') C') ∧
      mode.subtype t t' ∧ Caps.Sub (Ty.eff r ε t C).reach T.reach := by
  cases mode with
  | captures =>
    obtain ⟨_, _, _, _, he, hr, hε, ht, hc⟩ := Le.eff_left h
    exact ⟨_, _, _, _, he, ⟨hr, hε⟩, ht, hc⟩
  | live => exact LeLive.eff_left h

theorem TypingMode.subtype.eff_right_mode {mode : TypingMode} {r ε t C T}
    (h : mode.subtype T (.eff r ε t C)) :
    ∃ r' ε' t' C', T = .eff r' ε' t' C' ∧
      (mode.use (.rt r' ε') C').le (mode.use (.rt r ε) C) ∧
      mode.subtype t' t ∧ Caps.Sub T.reach (Ty.eff r ε t C).reach := by
  cases mode with
  | captures =>
    obtain ⟨_, _, _, _, he, hr, hε, ht, hc⟩ := Le.eff_right h
    exact ⟨_, _, _, _, he, ⟨hr, hε⟩, ht, hc⟩
  | live => exact LeLive.eff_right h

/-- Both disciplines relate two arrows componentwise: the target's argument is below
the source's, the source's use stage is below the target's, and so is its result. -/
theorem TypingMode.arrow_inv {mode : TypingMode} {A φ₀ t₀ C₀ a φ t C}
    (h : mode.subtype (.arrow A φ₀ t₀ C₀) (.arrow a φ t C)) :
    mode.subtype a A ∧ (mode.use φ₀ C₀).le (mode.use φ C) ∧ mode.subtype t₀ t := by
  cases mode with
  | captures =>
    cases h with
    | refl => exact ⟨.refl, Stage.le_refl _, .refl⟩
    | arrow h₁ h₂ h₃ => exact ⟨h₁, h₂, h₃⟩
  | live =>
    cases h with
    | refl => exact ⟨.refl, Stage.le_refl _, .refl⟩
    | arrow h₁ h₂ h₃ => exact ⟨h₁, h₂, h₃⟩

/-- The constant `op` asks for the region of its capability in the live discipline. -/
theorem Const.ty_op_arrow (S : Sig) (ι : CArg) (mode : TypingMode) :
    ∃ r, Const.ty S .op ι mode = .arrow (.hnd ι.s) r (.base .unit) Caps.none := by
  cases mode
  · exact ⟨_, rfl⟩
  · exact ⟨_, rfl⟩
/-- The first argument of `op` is the capability of its region. -/
theorem TypingMode.op_dom {mode : TypingMode} {S : Sig} {ι : CArg} {a ψ t C}
    (h : mode.subtype (Const.ty S .op ι mode) (.arrow a ψ t C)) : mode.subtype a (.hnd ι.s) := by
  obtain ⟨r, hr⟩ := Const.ty_op_arrow S ι mode
  rw [hr] at h
  exact (TypingMode.arrow_inv h).1
/-- A subtype of a type that is neither an arrow nor a computation is that type. -/
theorem TypingMode.subtype.eq_of {mode : TypingMode} {a b : Ty}
    (ha : ∀ x φ y C, b ≠ .arrow x φ y C) (he : ∀ r ε y C, b ≠ .eff r ε y C)
    (h : mode.subtype a b) : a = b := by
  cases mode with
  | captures =>
    cases h with
    | refl => rfl
    | arrow => exact absurd rfl (ha _ _ _ _)
    | eff => exact absurd rfl (he _ _ _ _)
  | live =>
    cases h with
    | refl => rfl
    | arrow => exact absurd rfl (ha _ _ _ _)
    | eff => exact absurd rfl (he _ _ _ _)

theorem TypingMode.subtype.eq_ref {mode : TypingMode} {a t : Ty} (h : mode.subtype a (.ref t)) :
    a = .ref t :=
  TypingMode.subtype.eq_of (fun _ _ _ _ h => by cases h) (fun _ _ _ _ h => by cases h) h

theorem TypingMode.subtype.eq_owner {mode : TypingMode} {a s : Ty} (h : mode.subtype a (.owner s)) :
    a = .owner s :=
  TypingMode.subtype.eq_of (fun _ _ _ _ h => by cases h) (fun _ _ _ _ h => by cases h) h

theorem TypingMode.subtype.eq_hnd {mode : TypingMode} {a s : Ty} (h : mode.subtype a (.hnd s)) :
    a = .hnd s :=
  TypingMode.subtype.eq_of (fun _ _ _ _ h => by cases h) (fun _ _ _ _ h => by cases h) h

theorem TypingMode.subtype.eq_base {mode : TypingMode} {a : Ty} {b : Base}
    (h : mode.subtype a (.base b)) : a = .base b :=
  TypingMode.subtype.eq_of (fun _ _ _ _ h => by cases h) (fun _ _ _ _ h => by cases h) h

/-- A type below one that is neither an arrow nor a computation is that type. -/
theorem TypingMode.subtype.eq_of_left {mode : TypingMode} {a b : Ty}
    (ha : ∀ x φ y C, a ≠ .arrow x φ y C) (he : ∀ r ε y C, a ≠ .eff r ε y C)
    (h : mode.subtype a b) : a = b := by
  cases mode with
  | captures =>
    cases h with
    | refl => rfl
    | arrow => exact absurd rfl (ha _ _ _ _)
    | eff => exact absurd rfl (he _ _ _ _)
  | live =>
    cases h with
    | refl => rfl
    | arrow => exact absurd rfl (ha _ _ _ _)
    | eff => exact absurd rfl (he _ _ _ _)

theorem TypingMode.subtype.eq_base_left {mode : TypingMode} {a : Ty} {b : Base}
    (h : mode.subtype (.base b) a) : a = .base b :=
  ((TypingMode.subtype.eq_of_left (a := .base b) (fun _ _ _ _ h => by cases h)
    (fun _ _ _ _ h => by cases h) h)).symm

theorem TypingMode.subtype.eq_ref_left {mode : TypingMode} {a t : Ty}
    (h : mode.subtype (.ref t) a) : a = .ref t :=
  ((TypingMode.subtype.eq_of_left (a := .ref t) (fun _ _ _ _ h => by cases h)
    (fun _ _ _ _ h => by cases h) h)).symm

theorem TypingMode.subtype.eq_hnd_left {mode : TypingMode} {a s : Ty}
    (h : mode.subtype (.hnd s) a) : a = .hnd s :=
  ((TypingMode.subtype.eq_of_left (a := .hnd s) (fun _ _ _ _ h => by cases h)
    (fun _ _ _ _ h => by cases h) h)).symm

theorem TypingMode.subtype.eq_owner_left {mode : TypingMode} {a s : Ty}
    (h : mode.subtype (.owner s) a) : a = .owner s :=
  ((TypingMode.subtype.eq_of_left (a := .owner s) (fun _ _ _ _ h => by cases h)
    (fun _ _ _ _ h => by cases h) h)).symm

theorem TypingMode.subtype.typ_ref {mode : TypingMode} {Ψ Θ Γ e x t φ}
    (h : mode.subtype x (.ref t)) (he : TypM S mode Ψ Θ Γ e (.ex x φ)) :
    TypM S mode Ψ Θ Γ e (.ex (.ref t) φ) := by
  cases TypingMode.subtype.eq_ref h; exact he

theorem TypingMode.subtype.typ_owner {mode : TypingMode} {Ψ Θ Γ e x s φ}
    (h : mode.subtype x (.owner s)) (he : TypM S mode Ψ Θ Γ e (.ex x φ)) :
    TypM S mode Ψ Θ Γ e (.ex (.owner s) φ) := by
  cases TypingMode.subtype.eq_owner h; exact he

theorem TypingMode.subtype.typ_hnd {mode : TypingMode} {Ψ Θ Γ e x s φ}
    (h : mode.subtype x (.hnd s)) (he : TypM S mode Ψ Θ Γ e (.ex x φ)) :
    TypM S mode Ψ Θ Γ e (.ex (.hnd s) φ) := by
  cases TypingMode.subtype.eq_hnd h; exact he

theorem TypingMode.subtype.typ_base {mode : TypingMode} {Ψ Θ Γ e x b φ}
    (h : mode.subtype x (.base b)) (he : TypM S mode Ψ Θ Γ e (.ex x φ)) :
    TypM S mode Ψ Θ Γ e (.ex (.base b) φ) := by
  cases TypingMode.subtype.eq_base h; exact he
end Flick
