import FormalCert.GenSig

/-!
# Coverage of the arms of a certificate

The `match`es of the generator cover their type by one of a few shapes: an arm `_` or `x`,
`()`, `true | false`, a constructor spine `K q₁ … qₙ` with each `qᵢ` a `_` or `x` for every
constructor of the type (`Some x | None`, a tuple `(x, y)`), or a record `{ℓ = x, ..}`. Each is
`Exh` (`Formal/Typing.lean`), as `exh_ctors` of `Elab/Exh.lean` is for the witness; a certificate
decides the premises with `decide`.
-/

namespace Flick.Cert

variable {S : Sig}

theorem Exh.mono {t : Ty} {ps ps' : List Pat} (h : Exh S t ps) (hs : ∀ p ∈ ps, p ∈ ps') :
    Exh S t ps' := fun v hv => by
  obtain ⟨p, hp, hm⟩ := h v hv
  exact ⟨p, hs p hp, hm⟩

theorem exh_wild {t : Ty} {ps : List Pat} (h : Pat.wild ∈ ps) : Exh S t ps :=
  fun _ _ => ⟨_, h, rfl⟩

theorem exh_var {t : Ty} {ps : List Pat} (h : Pat.var ∈ ps) : Exh S t ps :=
  fun _ _ => ⟨_, h, rfl⟩

theorem exh_unit : Exh S (.base .unit) [.lit .unit] := by
  intro v hs
  have : ∀ {fl T}, Shp S fl T v → T = .base .unit → fl = true → v = .lit .unit := by
    intro fl T h hT hf
    cases h with
    | @lit l => cases l <;> simp [Lit.base] at hT; rfl
    | hidden ho => subst hT; simp [Ty.Opaque, Ty.hd] at ho
    | head => cases hf
    | app => cases hf
    | rnil => cases hT
    | rcons => cases hT
    | adt _ _ h => subst hT; simp [Ty.hd] at h
  rw [this hs rfl rfl]
  exact ⟨_, List.mem_singleton_self _, by simp [matchP]⟩

/-- A pattern of a field that matches any value. -/
def anyPat : Pat → Bool
  | .wild | .var => true
  | _ => false

theorem matchP_any {q : Pat} (h : anyPat q = true) (v : Tm) : (matchP q v).isSome := by
  cases q <;> simp_all [anyPat, matchP]

/-- `K q₁ … qₙ` with fields any value matches: `some (K, n)`. -/
def spineOf : Pat → Option (Name × Nat)
  | .ctor K => some (K, 0)
  | .app p q => if anyPat q then (spineOf p).map fun c => (c.1, c.2 + 1) else none
  | _ => none

theorem matchP_spineOf : ∀ {p : Pat} {K : Name} {n : Nat}, spineOf p = some (K, n) →
    ∀ v : Tm, v.head = .ctor K → v.args = n → (matchP p v).isSome
  | .ctor K', K, n, h, v, hh, ha => by
    simp only [spineOf, Option.some.injEq, Prod.mk.injEq] at h
    obtain ⟨rfl, rfl⟩ := h
    cases v <;> simp_all [Tm.head, Tm.args, matchP]
  | .app p q, K, n, h, v, hh, ha => by
    simp only [spineOf] at h
    split at h
    · rename_i hq
      obtain ⟨⟨K', m⟩, hp, he⟩ := Option.map_eq_some_iff.1 h
      simp only [Prod.mk.injEq] at he
      obtain ⟨rfl, rfl⟩ := he
      cases v with
      | app f e =>
        simp only [Tm.args, Nat.add_right_cancel_iff] at ha
        obtain ⟨x, hx⟩ := Option.isSome_iff_exists.1 (matchP_spineOf hp f hh ha)
        obtain ⟨y, hy⟩ := Option.isSome_iff_exists.1 (matchP_any hq e)
        simp [matchP, hx, hy]
      | _ => simp [Tm.args] at ha
    · cases h
  | .wild, _, _, h, _, _, _ | .var, _, _, h, _, _, _ | .lit _, _, _, h, _, _, _
  | .rany, _, _, h, _, _, _ | .rfield .., _, _, h, _, _, _ | .or .., _, _, h, _, _, _ => by
    simp [spineOf] at h

/-- **The constructors of `T` cover it**: for each constructor `K` of `T`, an arm `K q₁ … qₙ`
of fields any value matches. -/
theorem exh_spines {T : Name} {ps : List Pat}
    (hall : ∀ K k ts, S.ctor K = some (k, ts, T) → ∃ p ∈ ps, spineOf p = some (K, ts.length))
    (us : List Ty) : Exh S (.tcon T us) ps := by
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
      obtain ⟨p, hp, hsp⟩ := hall _ _ _ hK'
      exact ⟨p, hp, matchP_spineOf hsp v hh (by simpa using hn)⟩

/-- An arm of `ps` is `K q₁ … qₙ` of fields any value matches. -/
def covered (ps : List Pat) (K : Name) (n : Nat) : Bool := ps.any (spineOf · == some (K, n))

/-- **The generator's data**: an arm for each constructor of one of its types covers it. -/
theorem exh_gen (hS : GenSig S) {T : Name} {ps : List Pat} {us : List Ty}
    (hT : genAdtCtors T ≠ []) (hall : ∀ c ∈ genAdtCtors T, covered ps c.1 c.2 = true) :
    Exh S (.tcon T us) ps :=
  exh_spines (fun K k ts h => by
    have := hall _ (genAdtCtors_complete _ _ _ _ (hS.closed _ _ _ _ h hT))
    simpa [covered] using this) us

/-- A record value of a type with field `l` has it, and `..` matches it. -/
theorem field_of_shp {R : Ty} {v : Tm} {l : Label} (hs : Shp S true R v)
    (hR : (R.field l).isSome) : (∃ x, v.field l = some x) ∧ (matchP .rany v).isSome := by
  generalize hfl : true = fl at hs
  induction hs with
  | @hidden T _ ho => cases T <;> simp_all [Ty.Opaque, Ty.hd, Ty.field]
  | lit => simp [Ty.field] at hR
  | rnil => simp [Ty.field] at hR
  | @rcons _ e _ _ l' _ _ _ ihr =>
    refine ⟨?_, by simp [matchP]⟩
    by_cases h : l' = l
    · exact ⟨e, by simp [Tm.field, h]⟩
    · simp only [Ty.field, h, if_false] at hR
      obtain ⟨⟨x, hx⟩, -⟩ := ihr hR rfl
      exact ⟨x, by simp [Tm.field, h, hx]⟩
  | head => cases hfl
  | app => cases hfl
  | @adt _ _ _ T' _ hT _ _ => cases T' <;> simp_all [Ty.hd, Ty.field]

/-- **`{ℓ = q, ..}` covers a record type with field `ℓ`**, `q` any value's pattern. -/
theorem exh_record {R : Ty} {l : Label} {q : Pat} {ps : List Pat} (hR : (R.field l).isSome)
    (hq : anyPat q = true) (hp : .rfield l q .rany ∈ ps) : Exh S R ps := by
  intro v hs
  obtain ⟨⟨x, hx⟩, hr⟩ := field_of_shp hs hR
  obtain ⟨a, ha⟩ := Option.isSome_iff_exists.1 (matchP_any hq x)
  obtain ⟨b, hb⟩ := Option.isSome_iff_exists.1 hr
  exact ⟨_, hp, by simp [matchP, hx, ha, hb]⟩

end Flick.Cert
