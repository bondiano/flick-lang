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

/-- **Л3.** The hole of a well-typed context is well-typed, its stage is bounded
as in `ERel`, and any term of its type can replace it. -/
theorem ctx_typing {Ψ} :
    ∀ (E : List Frame) {r t φ}, OkE E → ¬ Value r → Typ S Ψ [] (plug E r) (.ex t φ) →
      ∃ t' φ', Typ S Ψ [] r (.ex t' φ') ∧ ERel E φ' φ ∧ Refill S Ψ t' φ' (plug E) t φ ∧
        ELook S Ψ E
  | [], _, _, _, _, _, h =>
    ⟨_, _, h, ⟨fun n hn => .inl hn, fun _ _ h => h, fun h => h⟩, fun _ _ h' => h',
      fun _ _ h => by simp [lookupE] at h⟩
  | F :: E, r, _, _, hE, hr, h => by
    obtain ⟨hF, hE'⟩ := hE.cons
    obtain ⟨t₁, φ₁, h₁, rel₁, back₁, look₁⟩ := ctx_typing E hE' (not_value_plug hr F) h
    obtain ⟨t', φ', h', rel, backF, lookF⟩ := frame_typing hr F hF h₁
    refine ⟨t', φ', h', ⟨fun n hn => ?_, fun hT => ?_, fun hp => rel.pure (rel₁.pure hp)⟩,
      fun X r' hr' => back₁ X (F.plug r') (backF X r' hr'), fun N v hv => ?_⟩
    · rcases rel.svc n hn with h | h
      · rcases rel₁.svc n h with h | h
        · exact .inl h
        · exact .inr ((domE_cons F E n).mpr (.inr h))
      · exact .inr ((domE_cons F E n).mpr (.inl h))
    · exact (rel.err (hT F (by simp))).trans (rel₁.err fun G hG => hT G (by simp [hG]))
    · simp only [lookupE] at hv
      cases hl : F.lookup N with
      | some w => rw [hl] at hv; cases hv; exact lookF N _ hl
      | none => rw [hl] at hv; exact look₁ N v hv

end Flick
