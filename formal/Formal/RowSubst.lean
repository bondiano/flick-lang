import Formal.SubstPat

/-!
# Substitution in the rows of handlers

A substitution keeps what a handler gives its body and the obligations a region
discharges at its end, when it keeps the regions and sends atoms to atoms.
-/

namespace Flick

/-- A substitution keeps what a handler gives its body when it keeps the handler's region,
sends atoms to atoms and brings no names into rows, and the arguments the handler answers
become ones it answers after the substitution (`hA`), every one exactly when they were (`hU`). -/
theorem Row.hBody_substT {E : Name} {A A' : TyClass → Prop} {r : Row} {σ : Atom → Ty}
    (hn : NoNames σ) (hA : ∀ c, A c → A' (c.substT σ)) (hU : (∀ c, A c) ↔ ∀ c, A' c) :
    Row.Sub ((Row.hBody E A r).substT σ) (Row.hBody E A' (r.substT σ)) := by
  intro x hx
  cases x with
  | nm N c =>
    rw [Row.substT_nm hn] at hx
    obtain ⟨c₀, hx, rfl⟩ := hx
    exact hx.imp (fun h => ⟨h.1, hA _ h.2⟩) fun h => ⟨h.1, .inl ⟨c₀, h.2, rfl⟩⟩
  | ob N c a =>
    rcases hx with ⟨c₀, b, h, rfl, hb⟩ | ⟨v, _, _, _, _, hv, _⟩
    · exact .inl ⟨c₀, b, h, rfl, hb⟩
    · exact (hn v N c hv).elim
  | atm a =>
    rcases hx with ⟨b, hb, h⟩ | ⟨b, G', ⟨G, hb, -⟩, h⟩
    · exact .inl ⟨b, hb, h⟩
    · exact .inr ⟨b, G, hb, h⟩
  | rv c K =>
    obtain ⟨b, G', ⟨G, hb, h₁, h₂, h₃⟩, K', hc, rfl⟩ := hx
    refine ⟨fun n => G n ∨ K' n, ⟨b, G, hb, K', hc, rfl⟩, fun n hn => ?_, fun n hn => ?_,
      fun hu => .inl (h₃ (mt hU.mp hu))⟩
    · rcases hn with hn | hn
      · exact (h₁ n hn).imp .inl fun h => ⟨h.1, mt hU.mpr h.2⟩
      · exact .inl (.inr hn)
    · rcases hn with hn | hn
      · exact (h₂ n hn).imp .inl fun h => ⟨h.1, hU.mp h.2⟩
      · exact .inl (.inr hn)
  | obv v G a =>
    obtain ⟨w, G', b, hw, h, hb⟩ := hx
    exact ⟨w, G', b, hw, h, hb⟩

/-- A substitution keeps what a run-time handler gives its body, as `Row.hBody_substT`. -/
theorem Row.hP_substT {Θ : Facts} {q : Nat} {E : Name} {A A' : TyClass → Prop} {r : Row}
    {σ : Atom → Ty} (hq : σ (.r q) = .rg q) (hat : ∀ b a, (σ b).reach a → σ b = a.ty)
    (hn : NoNames σ) (hA : ∀ c, A c → A' (c.substT σ)) (hU : (∀ c, A c) ↔ ∀ c, A' c) :
    Row.Sub ((Row.hP Θ q E A r).substT σ) (Row.hP (Θ.substT σ) q E A' (r.substT σ)) := by
  intro x hx
  cases x with
  | ob N c a =>
    rcases hx with ⟨c₀, b, ⟨h₁, h₂⟩, rfl, hb⟩ | ⟨v, _, _, _, _, hv, _⟩
    · simp only [Row.hP]
      refine ⟨fun h => ?_, fun h => .inl ⟨c₀, b, h₂ h, rfl, hb⟩⟩
      have ho := (h₁ h).substT σ
      rw [Atom.ty_substT, hat b a hb] at ho
      intro k hk
      exact ho k ⟨.r q, rfl, by rw [hq]; exact hk⟩
    · exact (hn v N c hv).elim
  | nm N c =>
    exact Row.hBody_substT (E := E) (r := r) hn hA hU (.nm N c)
      (by simpa only [Row.substT, Row.hP] using hx)
  | atm a =>
    exact Row.hBody_substT (E := E) (r := r) hn hA hU (.atm a)
      (by simpa only [Row.substT, Row.hP] using hx)
  | rv c K =>
    exact Row.hBody_substT (E := E) (r := r) hn hA hU (.rv c K)
      (by simpa only [Row.substT, Row.hP] using hx)
  | obv v G a =>
    exact Row.hBody_substT (E := E) (r := r) hn hA hU (.obv v G a)
      (by simpa only [Row.substT, Row.hP] using hx)

theorem Row.disch_substT {a a' : Atom} {r : Row} {σ : Atom → Ty} (h : ∀ x, (σ a).reach x → x = a') :
    Row.Sub ((Row.disch a r).substT σ) (Row.disch a' (r.substT σ)) := by
  intro x hx
  cases x with
  | ob N c y =>
    obtain ⟨c₀, b, ⟨rfl, h₁⟩, hc, hb⟩ | ⟨v, G, b, ⟨rfl, h₁⟩, hG, hv, hb⟩ := hx
    · exact ⟨h y hb, .inl ⟨c₀, h₁, hc⟩⟩
    · exact ⟨h y hb, .inr ⟨v, G, h₁, hG, hv⟩⟩
  | obv w H y => obtain ⟨v, G, b, ⟨rfl, h₁⟩, hv, hb⟩ := hx; exact ⟨h y hb, v, G, h₁, hv⟩
  | nm N _ | atm _ | rv _ _ => simp [Row.substT, Row.disch] at hx

theorem Row.disch_substT_eq {a a' : Atom} {r : Row} {σ : Atom → Ty} (h : ∀ x, (σ a).reach x ↔ x = a')
    (hn : NoNames σ) : (Row.disch a r).substT σ = Row.disch a' (r.substT σ) := by
  funext x; apply propext
  cases x with
  | ob N c y =>
    simp only [Row.disch, Row.substT_nm hn]
    constructor
    · rintro (⟨c₀, b, ⟨rfl, h₁⟩, hc, hb⟩ | ⟨v, _, _, _, _, hv, _⟩)
      · exact ⟨(h y).mp hb, c₀, h₁, hc⟩
      · exact (hn v N c hv).elim
    · rintro ⟨rfl, c₀, h₁, hc⟩; exact .inl ⟨c₀, a, ⟨rfl, h₁⟩, hc, (h _).mpr rfl⟩
  | obv w H y =>
    simp only [Row.substT, Row.disch]
    constructor
    · rintro ⟨v, G, b, ⟨rfl, h₁⟩, hv, hb⟩; exact ⟨(h y).mp hb, v, G, h₁, hv⟩
    · rintro ⟨rfl, v, G, h₁, hv⟩; exact ⟨v, G, a, ⟨rfl, h₁⟩, hv, (h _).mpr rfl⟩
  | _ => simp [Row.substT, Row.disch]

/-- A body with no obligation about the effect of its handler has what the handler gives. -/
theorem Stage.hP_of_out {φ₀ : Stage} {E : Name} {A : TyClass → Prop} {r ε : Row} {Θ : Facts}
    {q : Nat} (h : φ₀.le (.rt (Row.hBody E A r) ε)) (hO : φ₀.ObOut (Row.single E)) :
    φ₀.le (.rt (Row.hP Θ q E A r) ε) := by
  cases φ₀ with
  | pure => trivial
  | rt r₀ e₀ =>
    refine ⟨fun x hx => ?_, h.2⟩
    have h₁ := h.1 x hx
    cases x with
    | ob N c a =>
      exact ⟨fun hN => ((hO.1 N c a hx).2 _ (by rw [hN]; rfl)).elim, fun _ => h₁⟩
    | _ => exact h₁

theorem Stage.ObOut.substT {φ₀ : Stage} {o : Row} (σ : Atom → Ty) (h : φ₀.ObOut o) :
    (φ₀.substT σ).ObOut (o.substT σ) := by
  have none : ∀ {o : Row} {N : Name}, o.Named → (∀ c, ¬ o (.nm N c)) →
      ∀ c, ¬ (o.substT σ) (.nm N c) :=
    fun ho h c hc => by
      rw [ho.substT_nm σ] at hc
      obtain ⟨c₀, hc, -⟩ := hc; exact h c₀ hc
  cases φ₀ with
  | pure => exact ⟨fun _ _ _ h => h.elim, fun _ _ _ h => h.elim⟩
  | rt r e =>
    refine ⟨fun N c a ha => ?_, fun w H a ⟨v, G, b, hb, _, _⟩ => ?_⟩
    · rcases ha with ⟨c₀, b, hb, -, -⟩ | ⟨v, G, b, hb, -, -, -⟩
      · obtain ⟨ho, hN⟩ := h.1 N c₀ b hb
        exact ⟨ho.substT σ, none ho hN⟩
      · obtain ⟨ho, hN⟩ := h.2 v G b hb
        exact ⟨ho.substT σ, none ho (hN N)⟩
    · obtain ⟨ho, hN⟩ := h.2 v G b hb
      exact ⟨ho.substT σ, fun N => none ho (hN N)⟩

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
