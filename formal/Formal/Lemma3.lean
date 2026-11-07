import Formal.Context

/-!
# L3: the effect of a context (core.md §4)
-/

namespace Flick

variable {S : Sig}

/-- How the stage of the hole relates to the stage of `E[e]`: services may
grow by the effects of handlers, errors only under a `try` frame, and a pure
context has a pure hole. -/
structure ERel (E : List Frame) (φ' φ : Stage) : Prop where
  svc : Row.Sub φ'.svc (Row.union φ.svc (domE E))
  err : (∀ F ∈ E, ¬ F.isTry) → Row.Sub φ'.err φ.err
  pure : φ = .pure → φ' = .pure

theorem List.map_up_up (Ψ : List Ty) (a b : Nat) :
    (Ψ.map (Ty.up a)).map (Ty.up b) = Ψ.map (Ty.up (a + b)) := by
  simp [Ty.up_up]

theorem liftT_add (K k : Nat) (σ : Atom → Ty) : liftT k (liftT K σ) = liftT (K + k) σ := by
  funext a
  cases a with
  | v i =>
    by_cases h₁ : i < k
    · simp [liftT, h₁, show i < K + k by omega]
    · by_cases h₂ : i - k < K
      · simp [liftT, h₁, h₂, show i < K + k by omega, Ty.up_def, Ty.substT, shiftV]; omega
      · simp only [liftT, h₁, h₂, if_false, show ¬ i < K + k by omega, Ty.up_up]
        congr 3; omega
  | r p => simp [liftT, Ty.up_up]

/-- **L3.** The hole of a well-typed context is well-typed, its stage is bounded
as in `ERel`, and any term of its type can replace it. Under `K` frames of
(Let-Gen) right-hand sides the hole is `K` variables deep, with the facts of
their schemes, and pure; otherwise it has the facts of the context. -/
theorem ctx_typing {Ψ Θ} :
    ∀ (E : List Frame) {r t φ}, OkE E → ¬ Value r → Typ S Ψ Θ [] (plug E r) (.ex t φ) →
      ∃ K Θ' t' φ', Typ S (Ψ.map (Ty.up K)) Θ' [] r (.ex t' φ') ∧ ERel E φ' φ ∧
        (φ' = .pure ∨ K = 0 ∧ Ext Θ Θ') ∧ Refill S Ψ Θ K Θ' t' φ' (plug E) t φ r
  | [], _, _, _, _, _, h =>
    ⟨0, Θ, _, _, by simpa using h, ⟨fun n hn => .inl hn, fun _ _ h => h, fun h => h⟩,
      .inr ⟨rfl, Ext.refl _⟩,
      fun _ _ _ _ h' => by rw [List.map_up_zero, liftT_zero] at h'; exact h'⟩
  | F :: E, r, _, _, hE, hr, h => by
    obtain ⟨hF, hE'⟩ := hE.cons
    obtain ⟨K₁, Θ₁, t₁, φ₁, h₁, rel₁, p₁, back₁⟩ := ctx_typing E hE' (not_value_plug hr F) h
    obtain ⟨k, Θ', t', φ', h', rel, p, backF⟩ := frame_typing hr F hF h₁
    rw [List.map_up_up] at h'
    refine ⟨K₁ + k, Θ', t', φ', h', ⟨fun n hn => ?_, fun hT => ?_, fun hp => rel.pure (rel₁.pure hp)⟩,
      ?_, fun σ hσ X r' hr' => back₁ σ hσ X (F.plug r') ?_⟩
    · rcases rel.svc n hn with h | h
      · rcases rel₁.svc n h with h | h
        · exact .inl h
        · exact .inr ((domE_cons F E n).mpr (.inr h))
      · exact .inr ((domE_cons F E n).mpr (.inl h))
    · exact (rel.err (hT F (by simp))).trans (rel₁.err fun G hG => hT G (by simp [hG]))
    · rcases p with hp | ⟨hk, hΘ, -⟩
      · exact .inl hp
      · rcases p₁ with hp₁ | ⟨hK₁, hΘ₁⟩
        · exact .inl (rel.pure hp₁)
        · subst hk hK₁
          exact .inr ⟨rfl, hΘ₁.trans hΘ⟩
    · have hσ' : Fits (liftT K₁ σ) (F.plug r) := (hσ.sub fun _ h => plug_has E h).lift K₁
      have := backF (liftT K₁ σ) hσ' (X.map (Ty.up K₁)) r' (by
        rw [liftT_add, shift_comm, ← List.map_append, List.map_up_up]; exact hr')
      rwa [shift_comm, ← List.map_append] at this

end Flick
