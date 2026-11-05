import Formal.Typing

/-!
# Rows, stages and subtyping: preorder lemmas
-/

namespace Flick

theorem Row.Sub.refl (r : Row) : Row.Sub r r := fun _ h => h

theorem Row.Sub.trans {a b c : Row} (h₁ : Row.Sub a b) (h₂ : Row.Sub b c) : Row.Sub a c :=
  fun n h => h₂ n (h₁ n h)

theorem Stage.le_refl : ∀ φ : Stage, φ.le φ
  | .pure => trivial
  | .rt r e => ⟨Row.Sub.refl r, Row.Sub.refl e⟩

theorem Stage.le_trans : ∀ {a b c : Stage}, a.le b → b.le c → a.le c
  | .pure, _, _, _, _ => trivial
  | .rt _ _, .pure, _, h, _ => h.elim
  | .rt _ _, .rt _ _, .pure, _, h => h.elim
  | .rt _ _, .rt _ _, .rt _ _, ⟨h₁, h₂⟩, ⟨g₁, g₂⟩ => ⟨h₁.trans g₁, h₂.trans g₂⟩

theorem Stage.pure_le (φ : Stage) : Stage.le .pure φ := trivial

theorem Stage.eq_pure : ∀ {φ : Stage}, φ.le .pure → φ = .pure
  | .pure, _ => rfl
  | .rt _ _, h => h.elim

theorem Stage.le_svc : ∀ {φ ψ : Stage}, φ.le ψ → Row.Sub φ.svc ψ.svc
  | .pure, _, _ => fun _ h => h.elim
  | .rt _ _, .pure, h => h.elim
  | .rt _ _, .rt _ _, h => h.1

theorem Stage.le_err : ∀ {φ ψ : Stage}, φ.le ψ → Row.Sub φ.err ψ.err
  | .pure, _, _ => fun _ h => h.elim
  | .rt _ _, .pure, h => h.elim
  | .rt _ _, .rt _ _, h => h.2

/-- A runtime stage is only below a runtime stage. -/
theorem Stage.rt_le {r e : Row} : ∀ {φ : Stage}, (Stage.rt r e).le φ →
    ∃ r' e', φ = .rt r' e' ∧ Row.Sub r r' ∧ Row.Sub e e'
  | .pure, h => h.elim
  | .rt _ _, h => ⟨_, _, rfl, h.1, h.2⟩

/-- (≤-Cap): a subtype reaches no more than its supertype. -/
theorem Le.reach {a b : Ty} (h : Le a b) : Caps.Sub a.reach b.reach := by
  cases h with
  | refl => exact Caps.Sub.refl _
  | arrow _ _ _ hc => exact hc
  | eff _ _ _ hc => exact hc

theorem Le.arrow_left {a φ t C T} (h : Le (.arrow a φ t C) T) :
    ∃ a' φ' t' C', T = .arrow a' φ' t' C' ∧ Le a' a ∧ φ.le φ' ∧ Le t t' ∧
      Caps.Sub (Ty.arrow a φ t C).reach T.reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, .refl, Stage.le_refl _, .refl, Caps.Sub.refl _⟩
  | arrow h₁ h₂ h₃ h₄ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃, h₄⟩

theorem Le.arrow_right {a φ t C T} (h : Le T (.arrow a φ t C)) :
    ∃ a' φ' t' C', T = .arrow a' φ' t' C' ∧ Le a a' ∧ φ'.le φ ∧ Le t' t ∧
      Caps.Sub T.reach (Ty.arrow a φ t C).reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, .refl, Stage.le_refl _, .refl, Caps.Sub.refl _⟩
  | arrow h₁ h₂ h₃ h₄ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃, h₄⟩

theorem Le.eff_left {r e t C T} (h : Le (.eff r e t C) T) :
    ∃ r' e' t' C', T = .eff r' e' t' C' ∧ Row.Sub r r' ∧ Row.Sub e e' ∧ Le t t' ∧
      Caps.Sub (Ty.eff r e t C).reach T.reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, Row.Sub.refl _, Row.Sub.refl _, .refl, Caps.Sub.refl _⟩
  | eff h₁ h₂ h₃ h₄ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃, h₄⟩

theorem Le.eff_right {r e t C T} (h : Le T (.eff r e t C)) :
    ∃ r' e' t' C', T = .eff r' e' t' C' ∧ Row.Sub r' r ∧ Row.Sub e' e ∧ Le t' t ∧
      Caps.Sub T.reach (Ty.eff r e t C).reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, Row.Sub.refl _, Row.Sub.refl _, .refl, Caps.Sub.refl _⟩
  | eff h₁ h₂ h₃ h₄ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃, h₄⟩

theorem Le.trans : ∀ {b a c : Ty}, Le a b → Le b c → Le a c
  | .arrow x φ y C, _, _, h₁, h₂ => by
    obtain ⟨a', φ', t', C', rfl, ha, hφ, ht, hc⟩ := h₁.arrow_right
    obtain ⟨a'', φ'', t'', C'', rfl, ga, gφ, gt, gc⟩ := h₂.arrow_left
    exact .arrow (ga.trans ha) (Stage.le_trans hφ gφ) (ht.trans gt) (hc.trans gc)
  | .eff r e t C, _, _, h₁, h₂ => by
    obtain ⟨_, _, _, _, rfl, hr, he, ht, hc⟩ := h₁.eff_right
    obtain ⟨_, _, _, _, rfl, gr, ge, gt, gc⟩ := h₂.eff_left
    exact .eff (hr.trans gr) (he.trans ge) (ht.trans gt) (hc.trans gc)
  | .base _, _, _, h₁, h₂ | .ref _, _, _, h₁, h₂ | .adt _, _, _, h₁, h₂
  | .tvar _, _, _, h₁, h₂ | .rnil, _, _, h₁, h₂ | .rcons _ _ _, _, _, h₁, h₂
  | .layer _ _ _, _, _, h₁, h₂ | .rg _, _, _, h₁, h₂ | .caps _, _, _, h₁, h₂ | .row _, _, _, h₁, h₂
  | .tapp _ _, _, _, h₁, h₂ | .hnd _, _, _, h₁, h₂ | .owner _, _, _, h₁, h₂
  | .errors _, _, _, h₁, h₂ | .env _, _, _, h₁, h₂ | .dict _ _, _, _, h₁, h₂ => by
    cases h₁; exact h₂

end Flick
