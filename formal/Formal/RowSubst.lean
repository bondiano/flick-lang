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
    rcases hx with ⟨b, ⟨h₁, h₂⟩, hb⟩ | ⟨v, _, _, hv, _⟩
    · simp only [Row.withP, Row.substT_nm hn]
      refine ⟨fun h => ?_, fun h => .inl ⟨b, h₂ h, hb⟩⟩
      have ho := (h₁ h).substT σ
      rw [Atom.ty_substT, hat b a hb] at ho
      intro k hk
      exact ho k ⟨.r q, rfl, by rw [hq]; exact hk⟩
    · exact (hn v N hv).elim
  | atm a | need N a | rv a | obv v a => exact hx

theorem Row.disch_substT {a a' : Atom} {r : Row} {σ : Atom → Ty} (h : ∀ x, (σ a).reach x → x = a') :
    Row.Sub ((Row.disch a r).substT σ) (Row.disch a' (r.substT σ)) := by
  intro x hx
  cases x with
  | ob N y =>
    obtain ⟨b, ⟨rfl, h₁⟩, hb⟩ | ⟨v, b, ⟨rfl, h₁⟩, hv, hb⟩ := hx
    · exact ⟨h y hb, .inl h₁⟩
    · exact ⟨h y hb, .inr ⟨v, h₁, hv⟩⟩
  | obv w y => obtain ⟨v, b, ⟨rfl, h₁⟩, hv, hb⟩ := hx; exact ⟨h y hb, v, h₁, hv⟩
  | nm N | atm _ | need _ _ | rv _ => simp [Row.substT, Row.disch] at hx

theorem Row.disch_substT_eq {a a' : Atom} {r : Row} {σ : Atom → Ty} (h : ∀ x, (σ a).reach x ↔ x = a')
    (hn : NoNames σ) : (Row.disch a r).substT σ = Row.disch a' (r.substT σ) := by
  funext x; apply propext
  cases x with
  | ob N y =>
    simp only [Row.disch, Row.substT_nm hn]
    constructor
    · rintro (⟨b, ⟨rfl, h₁⟩, hb⟩ | ⟨v, _, _, hv, _⟩)
      · exact ⟨(h y).mp hb, h₁⟩
      · exact (hn v N hv).elim
    · rintro ⟨rfl, h₁⟩; exact .inl ⟨a, ⟨rfl, h₁⟩, (h _).mpr rfl⟩
  | obv w y =>
    simp only [Row.substT, Row.disch]
    constructor
    · rintro ⟨v, b, ⟨rfl, h₁⟩, hv, hb⟩; exact ⟨(h y).mp hb, v, h₁, hv⟩
    · rintro ⟨rfl, v, h₁, hv⟩; exact ⟨v, a, ⟨rfl, h₁⟩, hv, (h _).mpr rfl⟩
  | _ => simp [Row.substT, Row.disch]

theorem Row.needs_substT {a a' : Atom} {σ : Atom → Ty} (h : ∀ x, (σ a).reach x → x = a') :
    Row.Sub ((Row.needs a).substT σ) (Row.needs a') := by
  intro x hx
  cases x with
  | need N y => obtain ⟨b, rfl, hb⟩ := hx; exact h y hb
  | nm N | atm _ | ob _ _ | rv _ | obv _ _ => simp [Row.substT, Row.needs] at hx

theorem Stage.ObOut.substT {φ₀ : Stage} {o : Row} (σ : Atom → Ty) (h : φ₀.ObOut o) :
    (φ₀.substT σ).ObOut (o.substT σ) := by
  have none : ∀ {o : Row}, o.Named → (∀ N, ¬ o (.nm N)) → ∀ N, ¬ (o.substT σ) (.nm N) :=
    fun ho h N => by rw [ho.substT_nm σ N]; exact h N
  cases φ₀ with
  | pure => exact ⟨fun _ _ h => h.elim, fun _ _ h => h.elim⟩
  | rt r e =>
    refine ⟨fun N a ha => ?_, fun w a ⟨v, b, hb, _, _⟩ => ?_⟩
    · rcases ha with ⟨b, hb, -⟩ | ⟨v, b, hb, -, -⟩
      · obtain ⟨ho, hN⟩ := h.1 N b hb
        exact ⟨ho.substT σ, by rw [ho.substT_nm σ N]; exact hN⟩
      · obtain ⟨ho, hN⟩ := h.2 v b hb
        exact ⟨ho.substT σ, none ho hN N⟩
    · obtain ⟨ho, hN⟩ := h.2 v b hb
      exact ⟨ho.substT σ, none ho hN⟩

end Flick
