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
    rcases hx with ⟨b, ⟨h₁, h₂⟩, hb⟩ | ⟨v, _, _, _, _, hv, _⟩
    · simp only [Row.hP]
      refine ⟨fun h => ?_, fun h => .inl ⟨b, h₂ h, hb⟩⟩
      have ho := (h₁ h).substT σ
      rw [Atom.ty_substT, hat b a hb] at ho
      intro k hk
      exact ho k ⟨.r q, rfl, by rw [hq]; exact hk⟩
    · exact (hn v N hv).elim
  | atm a =>
    rcases hx with ⟨b, hb, h⟩ | ⟨b, G', ⟨G, hb, -, -⟩, h⟩
    · exact .inl ⟨b, hb, h⟩
    · exact .inr ⟨b, G, hb, h⟩
  | rv c K =>
    obtain ⟨b, G', ⟨G, hb, hsub, hback⟩, K', hc, rfl⟩ := hx
    exact ⟨fun n => G n ∨ K' n, ⟨b, G, hb, K', hc, rfl⟩, fun n hn => hn.imp (hsub n) id,
      fun n hn => hn.elim (fun h => (hback n h).imp_left .inl) fun h => .inl (.inr h)⟩
  | obv v a => exact hx

theorem Row.disch_substT {a a' : Atom} {r : Row} {σ : Atom → Ty} (h : ∀ x, (σ a).reach x → x = a') :
    Row.Sub ((Row.disch a r).substT σ) (Row.disch a' (r.substT σ)) := by
  intro x hx
  cases x with
  | ob N y =>
    obtain ⟨b, ⟨rfl, h₁⟩, hb⟩ | ⟨v, G, b, ⟨rfl, h₁⟩, hG, hv, hb⟩ := hx
    · exact ⟨h y hb, .inl h₁⟩
    · exact ⟨h y hb, .inr ⟨v, G, h₁, hG, hv⟩⟩
  | obv w H y => obtain ⟨v, G, b, ⟨rfl, h₁⟩, hv, hb⟩ := hx; exact ⟨h y hb, v, G, h₁, hv⟩
  | nm N | atm _ | rv _ _ => simp [Row.substT, Row.disch] at hx

theorem Row.disch_substT_eq {a a' : Atom} {r : Row} {σ : Atom → Ty} (h : ∀ x, (σ a).reach x ↔ x = a')
    (hn : NoNames σ) : (Row.disch a r).substT σ = Row.disch a' (r.substT σ) := by
  funext x; apply propext
  cases x with
  | ob N y =>
    simp only [Row.disch, Row.substT_nm hn]
    constructor
    · rintro (⟨b, ⟨rfl, h₁⟩, hb⟩ | ⟨v, _, _, _, _, hv, _⟩)
      · exact ⟨(h y).mp hb, h₁⟩
      · exact (hn v N hv).elim
    · rintro ⟨rfl, h₁⟩; exact .inl ⟨a, ⟨rfl, h₁⟩, (h _).mpr rfl⟩
  | obv w H y =>
    simp only [Row.substT, Row.disch]
    constructor
    · rintro ⟨v, G, b, ⟨rfl, h₁⟩, hv, hb⟩; exact ⟨(h y).mp hb, v, G, h₁, hv⟩
    · rintro ⟨rfl, v, G, h₁, hv⟩; exact ⟨v, G, a, ⟨rfl, h₁⟩, hv, (h _).mpr rfl⟩
  | _ => simp [Row.substT, Row.disch]

/-- A body with no obligation about the effect of its handler has what the handler gives. -/
theorem Stage.hP_of_out {φ₀ : Stage} {E : Name} {r ε : Row} {Θ : Facts} {q : Nat}
    (h : φ₀.le (.rt (Row.union (Row.single E) (r.back (Row.single E))) ε))
    (hO : φ₀.ObOut (Row.single E)) :
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
  | pure => exact ⟨fun _ _ h => h.elim, fun _ _ _ h => h.elim⟩
  | rt r e =>
    refine ⟨fun N a ha => ?_, fun w H a ⟨v, G, b, hb, _, _⟩ => ?_⟩
    · rcases ha with ⟨b, hb, -⟩ | ⟨v, G, b, hb, -, -, -⟩
      · obtain ⟨ho, hN⟩ := h.1 N b hb
        exact ⟨ho.substT σ, by rw [ho.substT_nm σ N]; exact hN⟩
      · obtain ⟨ho, hN⟩ := h.2 v G b hb
        exact ⟨ho.substT σ, none ho hN N⟩
    · obtain ⟨ho, hN⟩ := h.2 v G b hb
      exact ⟨ho.substT σ, none ho hN⟩

/-- A region atom a stage carries is still carried after a substitution that keeps its
region. -/
theorem Stage.substT_svc_atm {σ : Atom → Ty} {p : Nat} {φ : Stage} (hp : σ (.r p) = .rg p)
    (h : φ.svc (.atm (.r p))) : (Stage.substT σ φ).svc (.atm (.r p)) := by
  cases φ with
  | pure => cases h
  | rt r e =>
    simp only [Stage.substT, Stage.svc, Row.substT]
    exact Or.inl ⟨.r p, h, by rw [hp]; rfl⟩


end Flick
