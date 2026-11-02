import Formal.CanonForms

/-!
# Arms `K x₁ … xₙ` for every constructor are exhaustive

The bodies of the witness match a value of a nominal type by one arm `K x₁ … xₙ` per
constructor, with no `_`. `Exh` holds for them (`exh_ctors`): a shape of a closed value of
`T ū` is a saturated spine of a constructor of `T` (`Shp.spine`), and its arm binds every field
to a variable, so the shapes of the fields do not matter. `exh_one` is the case of a type with
one constructor (`MkEff`, `MkRec`, the pair `Tup`).
-/

namespace Flick

variable {S : Sig}

/-- `K x₁ … xₙ`. -/
def pvars (K : Name) : Nat → Pat
  | 0 => .ctor K
  | n + 1 => .app (pvars K n) .var

/-- An unsaturated shape is a constructor spine, still waiting for `rest`. -/
theorem Shp.spine {fl : Bool} {v : Tm} {U : Ty} (h : Shp S fl U v) : fl = false →
    ∃ K k ts T rest, S.ctor K = some (k, ts, T) ∧ v.head = .ctor K ∧
      U = Ty.arrows rest (.tcon T (idTs k)) ∧ v.args + rest.length = ts.length := by
  induction h with
  | head hK => intro _; exact ⟨_, _, _, _, _, hK, rfl, rfl, by simp [Tm.args]⟩
  | app _ _ ihf _ =>
    intro _
    obtain ⟨K, k, ts, T, rest, hK, hh, hU, hn⟩ := ihf rfl
    cases rest with
    | nil =>
      have := congrArg Ty.hd hU
      simp [Ty.arrows, Ty.hd, Ty.tcon_hd] at this
    | cons a rest =>
      simp only [Ty.arrows, Ty.arrow.injEq] at hU
      obtain ⟨-, -, rfl, -⟩ := hU
      exact ⟨K, k, ts, T, rest, hK, hh, rfl, by simp only [Tm.args, List.length_cons] at hn ⊢; omega⟩
  | _ => intro h; cases h

theorem matchP_pvars (K : Name) : ∀ (n : Nat) (v : Tm), v.head = .ctor K → v.args = n →
    (matchP (pvars K n) v).isSome
  | 0, v, hh, ha => by
    cases v <;> simp_all [Tm.head, Tm.args, pvars, matchP]
  | n + 1, v, hh, ha => by
    cases v with
    | app f e =>
      simp only [Tm.args, Nat.add_right_cancel_iff] at ha
      obtain ⟨x, hx⟩ := Option.isSome_iff_exists.1 (matchP_pvars K n f hh ha)
      simp [pvars, matchP, hx]
    | _ => simp [Tm.args] at ha

/-- **The constructors of `T` cover it**: arms `K x₁ … xₙ`, one for each constructor of `T`. -/
theorem exh_ctors {T : Name} {ps : List Pat}
    (hall : ∀ K k ts, S.ctor K = some (k, ts, T) → pvars K ts.length ∈ ps) (us : List Ty) :
    Exh S (.tcon T us) ps := by
  intro v hs
  generalize hX : Ty.tcon T us = X at hs
  cases hs with
  | hidden ho => subst hX; simp [Ty.Opaque, Ty.tcon_hd] at ho
  | lit => have := congrArg Ty.hd hX; simp [Ty.hd, Ty.tcon_hd] at this
  | rnil => have := congrArg Ty.hd hX; simp [Ty.hd, Ty.tcon_hd] at this
  | rcons => have := congrArg Ty.hd hX; simp [Ty.hd, Ty.tcon_hd] at this
  | adt hs' hU hT =>
    subst hX
    rw [Ty.tcon_hd] at hT; cases hT
    obtain ⟨K', k', ts', T', rest, hK', hh, rfl, hn⟩ := hs'.spine rfl
    cases rest with
    | cons a rest => simp [Ty.arrows, Ty.hd] at hU
    | nil =>
      simp only [Ty.arrows, Ty.tcon_hd, Ty.adt.injEq] at hU
      subst hU
      exact ⟨_, hall _ _ _ hK', matchP_pvars _ _ v hh (by simpa using hn)⟩

/-- **A type with one constructor `K`**: `K x₁ … xₙ` covers it. -/
theorem exh_one {K : Name} {k : Nat} {ts : List Ty} {T : Name} (hK : S.ctor K = some (k, ts, T))
    (hone : ∀ K' k' ts', S.ctor K' = some (k', ts', T) → K' = K) (us : List Ty) :
    Exh S (.tcon T us) [pvars K ts.length] :=
  exh_ctors (fun K' k' ts' h => by
    obtain rfl := hone _ _ _ h; rw [hK] at h; cases h; exact List.mem_singleton_self _) us

end Flick
