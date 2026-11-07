import Formal.Preservation

/-!
# What a redex writes

The sites of a term built by a step are those of the terms it is built from: a
substitution, a field, an update, a match, a hole filled,
the arm of an operation (`perform_has`).
-/

namespace Flick

theorem subst_inst_has {e : Tm} {δ : List Tm} {s : Site} (h : (e.subst (inst δ)).Has s) :
    e.Has s ∨ ∃ w ∈ δ, w.Has s := by
  rcases Tm.has_subst e _ _ h with h | ⟨i, _, h⟩
  · exact .inl h
  · unfold inst at h
    split at h
    · exact .inr ⟨_, List.getElem_mem _, h⟩
    · simp [Tm.Has] at h

theorem field_has {v x : Tm} {l} {s : Site} (h : v.field l = some x) (hx : x.Has s) : v.Has s :=
  matchP_below.field_has h hx

theorem setField_has {v w v' : Tm} {l} {s : Site} (h : v.setField l w = some v') (hs : v'.Has s) :
    v.Has s ∨ w.Has s := by
  induction v generalizing v' with
  | rcons l' e r _ ih =>
    simp only [Tm.setField] at h
    split at h
    · cases h; rcases hs with hs | hs
      · exact .inr hs
      · exact .inl (.inr hs)
    · obtain ⟨r', hr', rfl⟩ := Option.map_eq_some_iff.mp h
      rcases hs with hs | hs
      · exact .inl (.inl hs)
      · exact (ih hr' hs).imp .inr id
  | _ => simp [Tm.setField] at h

theorem matchP_has {s : Site} : ∀ (p : Pat) {v : Tm} {δ : List Tm}, matchP p v = some δ →
    ∀ w ∈ δ, w.Has s → v.Has s
  | .wild, _, _, h => by simp [matchP] at h; subst h; simp
  | .var, _, _, h => by simp [matchP] at h; subst h; simp
  | .lit _, v, _, h => by cases v <;> simp [matchP] at h; obtain ⟨_, rfl⟩ := h; simp
  | .ctor _, v, _, h => by cases v <;> simp [matchP] at h; obtain ⟨_, rfl⟩ := h; simp
  | .app p q, v, _, h => by
    cases v with
    | app f e =>
      obtain ⟨δ₁, δ₂, h1, h2, rfl⟩ := matchP_app.mp h
      intro w hw hs
      rcases List.mem_append.mp hw with hw | hw
      · exact .inl (matchP_has p h1 w hw hs)
      · exact .inr (matchP_has q h2 w hw hs)
    | _ => simp [matchP] at h
  | .rany, v, _, h => by cases v <;> simp [matchP] at h <;> subst h <;> simp
  | .rfield l p r, v, _, h => by
    obtain ⟨x, δ₁, δ₂, hx, h1, h2, rfl⟩ := matchP_rfield.mp h
    intro w hw hs
    rcases List.mem_append.mp hw with hw | hw
    · exact field_has hx (matchP_has p h1 w hw hs)
    · exact matchP_has r h2 w hw hs
  | .or p q, v, _, h => by
    rcases matchP_or.mp h with h | ⟨_, h⟩
    · exact matchP_has p h
    · exact matchP_has q h

theorem subst_match_has {p : Pat} {v x : Tm} {δ : List Tm} {s : Site} (hm : matchP p v = some δ)
    (h : (x.subst (inst δ)).Has s) : x.Has s ∨ v.Has s :=
  (subst_inst_has h).imp_right fun ⟨w, hw, h⟩ => matchP_has _ hm w hw h

theorem Tm.armOf_has {o : Name} {s : Site} {b : Tm} : ∀ {al : Tm}, al.armOf o = some b → b.Has s → al.Has s := by
  intro al
  induction al with
  | acons p b' r _ ih =>
    intro h hs
    simp only [Tm.armOf] at h
    split at h
    · cases h; exact .inl hs
    · exact .inr (ih h hs)
  | _ => intro h; simp [Tm.armOf] at h

/-- What the step of an operation writes, its handler and the operation wrote: the
arm, the value, the context between them and the handler's region. -/
theorem perform_has {p ℓ : Nat} {Ef o : Name} {c : OpClass} {al r b v x : Tm} {Ei : List Frame}
    {δ : List Tm} {s : Site} (ha : al.armOf o = some b) (hδ : ∀ w ∈ δ, w.Has s → s = .rgn p ∨ v.Has s)
    (hx : x.Has s → s = .ins ∨ s = .rgn p ∨ (b.subst (inst δ)).Has s) :
    let e := Tm.hwith p ℓ Ef al r (plug Ei (.app (.const (.perform Ef o c)) v))
    (s = .ins → e.Has s) ∧ (s = .rgn p → e.Has s) ∧ (al.Has s → e.Has s) ∧ (r.Has s → e.Has s) ∧
      ((b.subst (inst δ)).Has s → e.Has s) ∧ ((plug Ei x).Has s → e.Has s) := by
  have h₅ : (b.subst (inst δ)).Has s →
      (Tm.hwith p ℓ Ef al r (plug Ei (.app (.const (.perform Ef o c)) v))).Has s := fun h => by
    rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact .inr (.inr (.inl (Tm.armOf_has ha h)))
    · rcases hδ w hw h with h | h
      · exact .inr (.inl h)
      · exact .inr (.inr (.inr (.inr (plug_has _ (by simp [Tm.Has, h])))))
  refine ⟨.inl, (.inr <| .inl ·), (.inr <| .inr <| .inl ·), (.inr <| .inr <| .inr <| .inl ·), h₅,
    fun h => ?_⟩
  rcases plug_has_inv (r := .app (.const (.perform Ef o c)) v) Ei h with h | h
  · exact .inr (.inr (.inr (.inr h)))
  · rcases hx h with h | h | h
    · exact .inl h
    · exact .inr (.inl h)
    · exact h₅ h

/-- A `tail` operation of a kept handler writes what the arm and the parked body write. -/
theorem kept_has {q ℓ : Nat} {Ef o : Name} {al b v : Tm} {Ei : List Frame} {s : Site}
    (ha : al.armOf o = some b)
    (h : (Tm.hkarm q ℓ Ef al (b.subst (inst [v])) (plug Ei (.hole q))).Has s) :
    (Tm.hkeep q ℓ Ef al (plug Ei (.app (.const (.perform Ef o .tail)) v))).Has s := by
  simp only [Tm.Has] at h ⊢
  rcases h with h | h | h | h | h
  · exact .inl h
  · exact .inr (.inl h)
  · exact .inr (.inr (.inl h))
  · rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact .inr (.inr (.inl (Tm.armOf_has ha h)))
    · simp at hw; rw [hw] at h; exact .inr (.inr (.inr (plug_has _ (.inr h))))
  · rcases plug_has_iff.mp h with h | h
    · exact .inr (.inr (.inr (plug_has_iff.mpr (.inl h))))
    · simp only [Tm.Has] at h
      rcases h with h | h
      · exact .inl h
      · exact .inr (.inl h)

end Flick
