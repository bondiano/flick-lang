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

theorem Tm.has_frame_ins {e : Tm} {p : Nat} (h : e.Has (.frame p)) : e.Has .ins := by
  induction e <;> simp_all [Tm.Has] <;> grind

theorem Tm.NoIns.frame {e : Tm} (h : e.NoIns) (p : Nat) : ¬ e.Has (.frame p) :=
  fun h' => h (Tm.has_frame_ins h')

theorem Tm.has_ins_site {e : Tm} (h : e.Has .ins) : ∃ p, e.Has (.rgn p) := by
  induction e <;> simp_all [Tm.Has] <;> grind

theorem Tm.Below.noIns {e : Tm} (h : e.Below 0) : e.NoIns := fun hi =>
  let ⟨p, hp⟩ := Tm.has_ins_site hi
  Nat.not_lt_zero _ (h p hp)


theorem Tm.Below.sub {n : Nat} {e e' : Tm} (h : e.Below n) (hs : ∀ s, e'.Has s → e.Has s) :
    e'.Below n := fun p hp => h p (hs _ hp)

theorem Tm.Below.mono {n n' : Nat} {e : Tm} (h : e.Below n) (hn : n ≤ n') : e.Below n' :=
  fun p hp => Nat.lt_of_lt_of_le (h p hp) hn

theorem Tm.Below.subst {n : Nat} {e : Tm} {δ : List Tm} (h : e.Below n) (hδ : ∀ v ∈ δ, v.Below n) :
    (e.subst (inst δ)).Below n := by
  have key : ∀ s, (e.subst (inst δ)).Has s → e.Has s ∨ ∃ w ∈ δ, w.Has s := fun s hp => by
    rcases Tm.has_subst e _ _ hp with h' | ⟨i, _, h'⟩
    · exact .inl h'
    · unfold inst at h'
      split at h'
      · exact .inr ⟨_, List.getElem_mem _, h'⟩
      · simp [Tm.Has] at h'
  intro p hp
  rcases key _ hp with h' | ⟨w, hw, h'⟩
  · exact h p h'
  · exact hδ w hw p h'

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
    | drec _ _ ih => exact ih h
    | _ => simp [Tm.field] at h

end Flick
