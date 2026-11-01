import Formal.PresRt

/-!
# A step writes only regions that exist

The terms of a configuration — its threads, the values in its store, its children
and finalizers — write only run-time regions `< |Λ|`. A step keeps this: it copies
what the term had, takes values from the store or from `Σ₀` (which has no regions,
`Typ.holds`), and a new scope writes the new region `|Λ|`.
-/

namespace Flick

variable {S : Sig}

theorem Frame.plug_has_inv {r r' : Tm} {s : Site} (F : Frame) (h : (F.plug r').Has s) :
    (F.plug r).Has s ∨ r'.Has s := by
  cases F <;> simp only [Frame.plug, Tm.Has] at h ⊢ <;> grind

theorem plug_has_inv {r r' : Tm} {s : Site} :
    ∀ (E : List Frame), (plug E r').Has s → (plug E r).Has s ∨ r'.Has s
  | [], h => .inr h
  | F :: E, h => by
    rcases plug_has_inv (r := F.plug r) E h with h | h
    · exact .inl h
    · rcases F.plug_has_inv (r := r) h with h | h
      · exact .inl (plug_has E h)
      · exact .inr h

theorem envLookup_has {θ v : Tm} {N : Name} {s : Site} (h : θ.envLookup N = some v) (hv : v.Has s) :
    θ.Has s := by
  induction θ with
  | econs N' w θ' _ ih =>
    simp only [Tm.envLookup] at h
    split at h
    · cases h; exact .inl hv
    · exact .inr (ih h)
  | _ => simp [Tm.envLookup] at h

theorem lookupE_has {r v : Tm} {N : Name} {s : Site} :
    ∀ (E : List Frame), lookupE E N = some v → v.Has s → (plug E r).Has s
  | [], h, _ => by simp [lookupE] at h
  | F :: E, h, hv => by
    simp only [lookupE] at h
    cases hF : F.lookup N with
    | some w =>
      rw [hF] at h; cases h
      cases F <;> simp [Frame.lookup] at hF
      exact plug_has E (show (Frame.plug (.with_ _) r).Has s from Or.inl (envLookup_has hF hv))
    | none => rw [hF] at h; exact lookupE_has (r := Frame.plug F r) E h hv

theorem Tm.Below.sub {n : Nat} {e e' : Tm} (h : e.Below n) (hs : ∀ p, e'.Has (.rgn p) → e.Has (.rgn p)) :
    e'.Below n := fun p hp => h p (hs p hp)

theorem Tm.Below.mono {n n' : Nat} {e : Tm} (h : e.Below n) (hn : n ≤ n') : e.Below n' :=
  fun p hp => Nat.lt_of_lt_of_le (h p hp) hn

theorem Tm.Below.subst {n : Nat} {e : Tm} {δ : List Tm} (h : e.Below n) (hδ : ∀ v ∈ δ, v.Below n) :
    (e.subst (inst δ)).Below n := by
  intro p hp
  rcases Tm.has_subst e _ _ hp with h' | ⟨i, _, h'⟩
  · exact h p h'
  · unfold inst at h'
    split at h'
    · exact hδ _ (List.getElem_mem _) p h'
    · simp [Tm.Has] at h'

theorem matchP_below {n : Nat} : ∀ (p : Pat) {v : Tm} {δ : List Tm}, v.Below n → matchP p v = some δ →
    ∀ w ∈ δ, w.Below n
  | .wild, _, _, _, h => by simp [matchP] at h; subst h; simp
  | .var, _, _, hv, h => by simp [matchP] at h; subst h; simpa using hv
  | .lit _, v, _, _, h => by cases v <;> simp [matchP] at h; obtain ⟨_, rfl⟩ := h; simp
  | .ctor _, v, _, _, h => by cases v <;> simp [matchP] at h; obtain ⟨_, rfl⟩ := h; simp
  | .app p q, v, _, hv, h => by
    cases v with
    | app f e =>
      obtain ⟨δ₁, δ₂, h1, h2, rfl⟩ := matchP_app.mp h
      intro w hw
      rcases List.mem_append.mp hw with hw | hw
      · exact matchP_below p (hv.sub fun _ h => .inl h) h1 w hw
      · exact matchP_below q (hv.sub fun _ h => .inr h) h2 w hw
    | _ => simp [matchP] at h
  | .rany, v, _, _, h => by cases v <;> simp [matchP] at h <;> subst h <;> simp
  | .rfield l p r, v, _, hv, h => by
    obtain ⟨x, δ₁, δ₂, hx, h1, h2, rfl⟩ := matchP_rfield.mp h
    have hxv : x.Below n := hv.sub fun q hq => field_has hx hq
    intro w hw
    rcases List.mem_append.mp hw with hw | hw
    · exact matchP_below p hxv h1 w hw
    · exact matchP_below r hv h2 w hw
  | .or p q, v, _, hv, h => by
    rcases matchP_or.mp h with h | ⟨_, h⟩
    · exact matchP_below p hv h
    · exact matchP_below q hv h
where
  field_has {v x : Tm} {l} {s : Site} (h : v.field l = some x) (hx : x.Has s) : v.Has s := by
    induction v with
    | rcons l' e r _ ih =>
      simp only [Tm.field] at h
      split at h
      · cases h; exact .inl hx
      · exact .inr (ih h)
    | _ => simp [Tm.field] at h

end Flick
