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

/-- A region atom carried by a stage is carried by every larger stage. -/
theorem Stage.atom_le {φ ψ : Stage} {p : Nat} (h : φ.le ψ) (ha : φ.svc (.atm (.r p))) :
    ψ.svc (.atm (.r p)) := Stage.le_svc h _ ha

theorem Stage.le_err : ∀ {φ ψ : Stage}, φ.le ψ → Row.Sub φ.err ψ.err
  | .pure, _, _ => fun _ h => h.elim
  | .rt _ _, .pure, h => h.elim
  | .rt _ _, .rt _ _, h => h.2

/-- A runtime stage is only below a runtime stage. -/
theorem Stage.rt_le {r e : Row} : ∀ {φ : Stage}, (Stage.rt r e).le φ →
    ∃ r' e', φ = .rt r' e' ∧ Row.Sub r r' ∧ Row.Sub e e'
  | .pure, h => h.elim
  | .rt _ _, h => ⟨_, _, rfl, h.1, h.2⟩

end Flick
