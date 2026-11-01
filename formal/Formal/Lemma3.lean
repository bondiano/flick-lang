import Formal.Context

/-!
# Л3: the effect of a context (core.md §4)
-/

namespace Flick

variable {S : Sig}

/-- How the stage of the hole relates to the stage of `E[e]`: services may
grow by those of the `with` frames, errors only under a `try` frame, and a
pure context has a pure hole. -/
structure ERel (E : List Frame) (φ' φ : Stage) : Prop where
  svc : Row.Sub φ'.svc (Row.union φ.svc (domE E))
  err : (∀ F ∈ E, ¬ F.isTry) → Row.Sub φ'.err φ.err
  pure : φ = .pure → φ' = .pure

/-- The services the `with` frames of `E` provide have their types. -/
def ELook (S : Sig) (Ψ : List Ty) (E : List Frame) : Prop :=
  ∀ N v, lookupE E N = some v → Typ S Ψ [] v (.ex (S.impl N) .pure)

theorem List.map_up_up (Ψ : List Ty) (a b : Nat) :
    (Ψ.map (Ty.up a)).map (Ty.up b) = Ψ.map (Ty.up (a + b)) := by
  simp [Ty.up_up]

/-- **Л3.** The hole of a well-typed context is well-typed, its stage is bounded
as in `ERel`, and any term of its type can replace it. Under `K` frames of
(Let-Gen) right-hand sides the hole is `K` type variables deep, and pure. -/
theorem ctx_typing {Ψ} :
    ∀ (E : List Frame) {r t φ}, OkE E → ¬ Value r → Typ S Ψ [] (plug E r) (.ex t φ) →
      ∃ K t' φ', Typ S (Ψ.map (Ty.up K)) [] r (.ex t' φ') ∧ ERel E φ' φ ∧
        (K ≠ 0 → φ' = .pure) ∧ Refill S Ψ K t' φ' (plug E) t φ ∧ (K = 0 → ELook S Ψ E)
  | [], _, _, _, _, _, h =>
    ⟨0, _, _, by simpa using h, ⟨fun n hn => .inl hn, fun _ _ h => h, fun h => h⟩,
      fun h => (h rfl).elim, fun _ _ h' => by simpa using h',
      fun _ _ _ h => by simp [lookupE] at h⟩
  | F :: E, r, _, _, hE, hr, h => by
    obtain ⟨hF, hE'⟩ := hE.cons
    obtain ⟨K₁, t₁, φ₁, h₁, rel₁, p₁, back₁, look₁⟩ := ctx_typing E hE' (not_value_plug hr F) h
    obtain ⟨k, t', φ', h', rel, p, backF, lookF⟩ := frame_typing hr F hF h₁
    rw [List.map_up_up] at h'
    refine ⟨K₁ + k, t', φ', h', ⟨fun n hn => ?_, fun hT => ?_, fun hp => rel.pure (rel₁.pure hp)⟩,
      fun hK => ?_, fun X r' hr' => back₁ X (F.plug r') ?_, fun hK N v hv => ?_⟩
    · rcases rel.svc n hn with h | h
      · rcases rel₁.svc n h with h | h
        · exact .inl h
        · exact .inr ((domE_cons F E n).mpr (.inr h))
      · exact .inr ((domE_cons F E n).mpr (.inl h))
    · exact (rel.err (hT F (by simp))).trans (rel₁.err fun G hG => hT G (by simp [hG]))
    · by_cases hk : k = 0
      · subst hk; exact rel.pure (p₁ (by omega))
      · exact p hk
    · have := backF (X.map (Ty.up K₁)) r' (by rw [← List.map_append, List.map_up_up]; exact hr')
      rwa [← List.map_append] at this
    · have hK₁ : K₁ = 0 := by omega
      have hk : k = 0 := by omega
      subst hK₁ hk
      simp only [lookupE] at hv
      cases hl : F.lookup N with
      | some w => rw [hl] at hv; cases hv; simpa using lookF N _ hl
      | none => rw [hl] at hv; exact look₁ rfl N v hv

end Flick
