import Formal.SubstPat

/-!
# Substitution in the rows of `provide`

A substitution keeps what a `with` frame gives its body, the obligations a region
discharges at its end and the uses of the services of a `provide` region, when it
keeps the regions involved and sends atoms to atoms.
-/

namespace Flick

theorem Atom.ty_substT (a : Atom) (σ : Atom → Ty) : a.ty.substT σ = σ a := by
  cases a <;> rfl

/-- A substitution keeps what a `with` frame gives its body when it keeps the frame's
region, sends atoms to atoms and brings no names into rows. -/
theorem Row.withP_substT {Θ : Facts} {q : Nat} {ρ r : Row} {σ : Atom → Ty} (hq : σ (.r q) = .rg q)
    (hat : ∀ b a, (σ b).reach a → σ b = a.ty) (hn : NoNames σ) :
    Row.Sub ((Row.withP Θ q ρ r).substT σ) (Row.withP (Θ.substT σ) q (ρ.substT σ) (r.substT σ)) := by
  intro x hx
  cases x with
  | nm N =>
    rw [Row.substT_nm hn] at hx
    simp only [Row.withP, Row.substT_nm hn]
    exact ⟨fun h => ⟨.r q, hx.1 h, by rw [hq]; rfl⟩, hx.2⟩
  | ob N a =>
    obtain ⟨b, ⟨h₁, h₂⟩, hb⟩ := hx
    simp only [Row.withP, Row.substT_nm hn]
    refine ⟨fun h => ?_, fun h => ⟨b, h₂ h, hb⟩⟩
    have ho := (h₁ h).substT σ
    rw [Atom.ty_substT, hat b a hb] at ho
    intro k hk
    exact ho k ⟨.r q, rfl, by rw [hq]; exact hk⟩
  | atm a | need N a | rv a => exact hx

theorem Row.disch_substT {a a' : Atom} {r : Row} {σ : Atom → Ty} (h : ∀ x, (σ a).reach x → x = a') :
    Row.Sub ((Row.disch a r).substT σ) (Row.disch a' (r.substT σ)) := by
  intro x hx
  cases x with
  | ob N y => obtain ⟨b, ⟨rfl, h₁⟩, hb⟩ := hx; exact ⟨h y hb, .inl h₁⟩
  | nm N | atm _ | need _ _ | rv _ => simp [Row.substT, Row.disch] at hx

theorem Row.disch_substT_eq {a a' : Atom} {r : Row} {σ : Atom → Ty} (h : ∀ x, (σ a).reach x ↔ x = a')
    (hn : NoNames σ) : (Row.disch a r).substT σ = Row.disch a' (r.substT σ) := by
  funext x; apply propext
  cases x with
  | ob N y =>
    simp only [Row.disch, Row.substT_nm hn]
    constructor
    · rintro ⟨b, ⟨rfl, h₁⟩, hb⟩; exact ⟨(h y).mp hb, h₁⟩
    · rintro ⟨rfl, h₁⟩; exact ⟨a, ⟨rfl, h₁⟩, (h _).mpr rfl⟩
  | _ => simp [Row.substT, Row.disch]

theorem Row.needs_substT {a a' : Atom} {σ : Atom → Ty} (h : ∀ x, (σ a).reach x → x = a') :
    Row.Sub ((Row.needs a).substT σ) (Row.needs a') := by
  intro x hx
  cases x with
  | need N y => obtain ⟨b, rfl, hb⟩ := hx; exact h y hb
  | nm N | atm _ | ob _ _ | rv _ => simp [Row.substT, Row.needs] at hx

theorem ob_free_substT {φ₀ : Stage} {o : Row} {σ : Atom → Ty} (ho : o.Named)
    (h : ∀ N a, φ₀.svc (.ob N a) → ¬ o (.nm N)) :
    ∀ N a, (φ₀.substT σ).svc (.ob N a) → ¬ (o.substT σ) (.nm N) := by
  intro N a ha
  rw [ho.substT_nm σ N]
  cases φ₀ with
  | pure => exact ha.elim
  | rt r e => obtain ⟨b, hb, -⟩ := ha; exact h N b hb

end Flick
