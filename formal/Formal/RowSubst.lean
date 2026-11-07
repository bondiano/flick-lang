import Formal.SubstPat

/-!
# Substitution in the rows of handlers

A substitution keeps what a handler gives its body and the obligations a region
discharges at its end, when it keeps the regions and sends atoms to atoms.
-/

namespace Flick

/-- A substitution keeps what a handler gives its body when it keeps the handler's region,
sends atoms to atoms and brings no names into rows. -/
theorem Row.hP_substT {Θ : Facts} {q : Nat} {E : Name} {r : Row} {σ : Atom → Ty} (hq : σ (.r q) = .rg q)
    (hat : ∀ b a, (σ b).reach a → σ b = a.ty) (hn : NoNames σ) :
    Row.Sub ((Row.hP Θ q E r).substT σ) (Row.hP (Θ.substT σ) q E (r.substT σ)) := by
  intro x hx
  cases x with
  | nm N =>
    rw [Row.substT_nm hn] at hx
    simp only [Row.hP, Row.substT_nm hn]
    exact hx
  | ob N a =>
    rcases hx with ⟨b, ⟨h₁, h₂⟩, hb⟩ | ⟨v, _, _, hv, _⟩
    · simp only [Row.hP]
      refine ⟨fun h => ?_, fun h => .inl ⟨b, h₂ h, hb⟩⟩
      have ho := (h₁ h).substT σ
      rw [Atom.ty_substT, hat b a hb] at ho
      intro k hk
      exact ho k ⟨.r q, rfl, by rw [hq]; exact hk⟩
    · exact (hn v N hv).elim
  | atm a | rv a | obv v a => exact hx

theorem Row.disch_substT {a a' : Atom} {r : Row} {σ : Atom → Ty} (h : ∀ x, (σ a).reach x → x = a') :
    Row.Sub ((Row.disch a r).substT σ) (Row.disch a' (r.substT σ)) := by
  intro x hx
  cases x with
  | ob N y =>
    obtain ⟨b, ⟨rfl, h₁⟩, hb⟩ | ⟨v, b, ⟨rfl, h₁⟩, hv, hb⟩ := hx
    · exact ⟨h y hb, .inl h₁⟩
    · exact ⟨h y hb, .inr ⟨v, h₁, hv⟩⟩
  | obv w y => obtain ⟨v, b, ⟨rfl, h₁⟩, hv, hb⟩ := hx; exact ⟨h y hb, v, h₁, hv⟩
  | nm N | atm _ | rv _ => simp [Row.substT, Row.disch] at hx

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

/-- A body with no obligation about the effect of its handler has what the handler gives. -/
theorem Stage.hP_of_out {φ₀ : Stage} {E : Name} {r ε : Row} {Θ : Facts} {q : Nat}
    (h : φ₀.le (.rt (Row.union (Row.single E) r) ε)) (hO : φ₀.ObOut (Row.single E)) :
    φ₀.le (.rt (Row.hP Θ q E r) ε) := by
  cases φ₀ with
  | pure => trivial
  | rt r₀ e₀ =>
    refine ⟨fun x hx => ?_, h.2⟩
    have h₁ := h.1 x hx
    cases x with
    | nm N =>
      rcases h₁ with h₁ | h₁
      · cases h₁; exact .inl rfl
      · exact .inr h₁
    | ob N a =>
      refine ⟨fun hN => ((hO.1 N a hx).2 (by rw [hN]; rfl)).elim, fun _ => ?_⟩
      rcases h₁ with h₁ | h₁
      · cases h₁
      · exact h₁
    | atm a | rv a | obv v a =>
      rcases h₁ with h₁ | h₁
      · cases h₁
      · exact h₁

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
