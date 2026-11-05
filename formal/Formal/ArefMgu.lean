import Formal.TySubst

/-!
# An arm on a constructor of a GADT, as the checker types it

The rule (Arm) on a constructor of a GADT types the arm under every unifier that
fits it. The checker types it once, under the most general unifier, or finds no
unifier and drops the arm. Both are instances of the rule: each other unifier is the
most general one followed by some `κ`, and type substitution carries the arm along.
-/

namespace Flick

variable {S : Sig}

/-- `θ` unifies `u` with `w`, keeps the regions `b` writes, and every other such
unifier is `θ` followed by some `κ`. -/
def Mgu (b : Tm) (θ : Atom → Ty) (u w : Ty) : Prop :=
  Fits θ b ∧ u.substT θ = w.substT θ ∧
    ∀ θ', Fits θ' b → u.substT θ' = w.substT θ' → ∃ κ : Atom → Ty, ∀ a, θ' a = (θ a).substT κ

theorem PatsTy.substT {qs xs Δ} (σ : Atom → Ty) (h : PatsTy S qs xs Δ) :
    PatsTy S qs (xs.map (Ty.substT σ)) (Δ.map (Ty.substT σ)) := by
  induction h with
  | nil => exact .nil
  | cons hq _ ih => simpa [List.map_append] using PatsTy.cons (hq.substT σ) ih

open Classical in
/-- (Arm) under the most general unifier. -/
theorem Typ.aref_mgu {Ψ Θ Γ K k ts T₀ qs b r t t' φ Δ θ}
    (hK : S.ctor K = some (k, ts, T₀)) (hU : t.hd = .adt T₀) (hn : b.NoIns)
    (hfr : ∀ i, b.Free i → i < (Pat.spine K qs).binds + Γ.length)
    (hlc : ∀ l, b.Has (.loc l) → l < Ψ.length)
    (hθ : Mgu b θ (Ty.tcon T₀ (S.res K)) (t.up k))
    (hp : PatsTy S qs (ts.map (Ty.substT θ)) Δ)
    (hb : Typ S ((Ψ.map (Ty.up k)).map (Ty.substT θ)) ((Θ.substT (shiftV k)).substT θ)
      (Δ.map .mono ++ (Γ.map (Scheme.up k)).map (Scheme.substT θ)) b
      (.ex ((t'.up k).substT θ) ((φ.substT (shiftV k)).substT θ)))
    (hr : Typ S Ψ Θ Γ r (.alts t t' φ)) :
    Typ S Ψ Θ Γ (.acons (Pat.spine K qs) b r) (.alts t t' φ) := by
  refine .aref (Δ := fun θ' =>
      if h : Fits θ' b ∧ (Ty.tcon T₀ (S.res K)).substT θ' = (t.up k).substT θ' then
        Δ.map (Ty.substT (Classical.choose (hθ.2.2 θ' h.1 h.2)))
      else []) hK hU hn hfr hlc (fun θ' hF hu => ?_) (fun θ' hF hu => ?_) hr
  all_goals
    simp only [dif_pos (And.intro hF hu)]
    have hκ := Classical.choose_spec (hθ.2.2 θ' hF hu)
    generalize Classical.choose (hθ.2.2 θ' hF hu) = κ at hκ ⊢
    obtain rfl : θ' = fun a => (θ a).substT κ := funext hκ
  · simpa [List.map_map, Function.comp_def, Ty.substT_comp] using hp.substT κ
  · have hκF : Fits κ b := ⟨fun p hp => by
        have h₁ := hF.1 p hp
        simp only [hθ.1.1 p hp] at h₁
        simpa [Ty.substT] using h₁,
      fun hi => (hn hi).elim, fun hi => (hn hi).elim, fun hi => (hn hi).elim⟩
    have := hb.substT κ hκF
    rw [ctx_substT] at this
    simpa [List.map_map, Function.comp_def, Ty.substT_comp, Scheme.substT_comp,
      Facts.substT_comp, Stage.substT_comp, Flick.J.substT] using this

/-- (Arm) on a constructor that builds no value of the matched type: the body is not
typed. -/
theorem Typ.aref_dead {Ψ Θ Γ K k ts T₀ qs b r t t' φ}
    (hK : S.ctor K = some (k, ts, T₀)) (hU : t.hd = .adt T₀) (hn : b.NoIns)
    (hfr : ∀ i, b.Free i → i < (Pat.spine K qs).binds + Γ.length)
    (hlc : ∀ l, b.Has (.loc l) → l < Ψ.length)
    (hd : ∀ θ, Fits θ b → (Ty.tcon T₀ (S.res K)).substT θ ≠ (t.up k).substT θ)
    (hr : Typ S Ψ Θ Γ r (.alts t t' φ)) :
    Typ S Ψ Θ Γ (.acons (Pat.spine K qs) b r) (.alts t t' φ) :=
  .aref (Δ := fun _ => []) hK hU hn hfr hlc (fun θ hF hu => (hd θ hF hu).elim)
    (fun θ hF hu => (hd θ hF hu).elim) hr

end Flick
