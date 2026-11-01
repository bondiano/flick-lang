import Formal.RT.Inv

-- The step tactics share one `simp` set across ~50 rules; not every rule needs every lemma.
set_option linter.unusedSimpArgs false

namespace Flick.RT

variable {T V : Type} {K : Core T V} {L : Lib V} {C C' : Conf T V}

theorem lookup_lt {M : Nat → Option (Memo V)} {Ly fuel m m' : Nat} {e : Ent V}
    (h : lookup M Ly fuel m = some (m', e)) : ∃ mm, M m' = some mm ∧ mm.ents Ly = some e := by
  induction fuel generalizing m with
  | zero => simp [lookup] at h
  | succ n ih =>
    simp only [lookup] at h
    split at h
    · cases h
    · next mm hm =>
      split at h
      · next e' he => cases h; exact ⟨mm, hm, he⟩
      · split at h
        · exact ih h
        · cases h

theorem lookup_none {M : Nat → Option (Memo V)} {Ly fuel m : Nat} {mm : Memo V}
    (h : lookup M Ly fuel m = none) (hf : 0 < fuel) (hm : M m = some mm) : mm.ents Ly = none := by
  cases fuel with
  | zero => omega
  | succ n =>
    simp only [lookup, hm] at h
    split at h
    · cases h
    · assumption

theorem held_rest {fr : Frame T V} {s : List (Frame T V)} {π : Nat}
    (h : ((fr :: s).filterMap Frame.held).Nodup) (hfr : fr.held = some π) :
    ∀ fr' ∈ s, fr'.held ≠ some π := by
  intro fr' hfr' h'
  rw [List.filterMap_cons, hfr, List.nodup_cons] at h
  exact h.1 (List.mem_filterMap.2 ⟨fr', hfr', h'⟩)

/-- A local step keeps who the fiber is. -/
theorem Local.same {L : Lib V} {φ φ' : Fiber T V} (h : Local K L C φ φ') :
    φ'.base = φ.base ∧ φ'.role = φ.role ∧ φ'.θ = φ.θ ∧ φ'.mm = φ.mm ∧ φ'.q = φ.q := by
  cases h <;> exact ⟨rfl, rfl, rfl, rfl, rfl⟩

/-- A local step pushes only frames that hold nothing. -/
theorem Local.frames {L : Lib V} {φ φ' : Fiber T V} (h : Local K L C φ φ') :
    ∀ fr ∈ φ'.stack, fr ∈ φ.stack ∨ (fr.held = none ∧ ∀ π, fr ≠ .closing π) := by
  cases h with
  | resume _ hs | then_ _ hs | acqFail _ hs | parDone _ hs | parEarly _ _ hs =>
    intro fr hfr; exact .inl (by rw [hs]; exact List.mem_cons_of_mem _ hfr)
  | acquire | par =>
    intro fr hfr
    simp only [List.mem_cons] at hfr
    rcases hfr with rfl | rfl | hfr
    · exact .inr ⟨rfl, fun _ h => by cases h⟩
    · exact .inr ⟨rfl, fun _ h => by cases h⟩
    · exact .inl hfr
  | join | modRead =>
    intro fr hfr
    simp only [List.mem_cons] at hfr
    rcases hfr with rfl | hfr
    · exact .inr ⟨rfl, fun _ h => by cases h⟩
    · exact .inl hfr
  | _ => exact fun fr h => .inl h

/-- Split a hypothesis `C'.Λ π = some o` along the updates of a step. -/
macro "owner_cases" h:ident : tactic => `(tactic| (
  (try simp only [upd, updO] at $h:ident) <;> (repeat' split at $h:ident) <;>
  (try simp only [Option.some.injEq, Option.map_some, Option.map_none, reduceCtorEq,
    Option.map_eq_some_iff] at $h:ident) <;>
  (try obtain ⟨o₀, h₀, $h:ident⟩ := $h:ident) <;> (try subst $h:ident)))

/-- Split a hypothesis `C'.Φ j = some s` along the updates of a step. -/
macro "fiber_cases" h:ident : tactic => `(tactic| (
  (try simp only [upd, setF] at $h:ident) <;> (repeat' split at $h:ident) <;>
  (try simp only [Option.some.injEq, FState.run.injEq, reduceCtorEq] at $h:ident) <;>
  (try subst $h:ident) <;> (try subst_vars)))

namespace Inv

variable (I : Inv C)
include I

theorem ltΦ {j : Nat} {s : FState T V} (h : C.Φ j = some s) : j < C.n :=
  Nat.lt_of_not_le fun hj => by simp [(I.fresh j hj).1] at h

theorem ltΛ {π : Nat} {o : Owner T V} (h : C.Λ π = some o) : π < C.n :=
  Nat.lt_of_not_le fun hj => by simp [(I.fresh π hj).2.1] at h

theorem ltG {γ : Nat} {g : Grp} (h : C.G γ = some g) : γ < C.n :=
  Nat.lt_of_not_le fun hj => by simp [(I.fresh γ hj).2.2.1] at h

theorem ltX {p π : Nat} (h : C.X p = some π) : p < C.n :=
  Nat.lt_of_not_le fun hj => by simp [(I.fresh p hj).2.2.2.1] at h

theorem ltM {m : Nat} {mm : Memo V} (h : C.M m = some mm) : m < C.n :=
  Nat.lt_of_not_le fun hj => by simp [(I.fresh m hj).2.2.2.2] at h

/-- The current owner of a running fiber is live. -/
theorem cur_live {j : Nat} {φ : Fiber T V} (h : C.Φ j = some (.run φ)) :
    ∃ o, C.Λ φ.cur = some o ∧ o.st.live = true := by
  have hfr := I.frames j φ h
  unfold Fiber.cur
  generalize φ.stack = s at hfr
  induction s with
  | nil => exact I.live j φ h
  | cons fr s ih =>
    cases fr with
    | inside π env =>
      obtain ⟨_, o, ho, hst, _⟩ := (hfr _ List.mem_cons_self).inside
      exact ⟨o, ho, by simp [hst, St.live]⟩
    | build πp m Ly σ Ls b =>
      obtain ⟨_, ⟨o, ho, hst, _⟩, _⟩ := (hfr _ List.mem_cons_self).build
      exact ⟨o, ho, by simp [hst, St.live]⟩
    | _ => exact ih fun fr h => hfr fr (List.mem_cons_of_mem _ h)

/-- An event about a fresh id is not in the journal. -/
theorem fresh_ev {e : Ev V} (h : C.n ∈ e.ids) : e ∉ C.J := fun he =>
  Nat.lt_irrefl _ (I.evIds e he _ h)

theorem run_fins {π i : Nat} (h : Ev.run π i ∈ C.J) : Ev.fins π ∈ C.J := by
  obtain ⟨J₂, hg, hsub⟩ := hist_mem I.hist h
  exact hsub _ hg.2.2.2.1

theorem fin_unique {π π' i : Nat} (h : Ev.fin π i ∈ C.J) (h' : Ev.fin π' i ∈ C.J) : π = π' := by
  have hh := I.hist
  obtain ⟨J₁, J₂, hJ⟩ := List.mem_iff_append.1 h
  have hg := hh J₁ _ J₂ hJ
  rw [hJ] at h'
  rcases List.mem_append.1 h' with h' | h'
  · obtain ⟨A, B, rfl⟩ := List.mem_iff_append.1 h'
    have hg' := hh A (Ev.fin π' i) (B ++ Ev.fin π i :: J₂) (by rw [hJ]; simp)
    exact absurd (by simp) (hg'.2.2.1 π)
  · rcases List.mem_cons.1 h' with h' | h'
    · cases h'; rfl
    · exact absurd h' (hg.2.2.1 π')

theorem fin_lt {π : Nat} {o : Owner T V} {t : Fin T} (ho : C.Λ π = some o)
    (ht : t ∈ o.F ++ o.fs) : t.id < C.n :=
  I.evIds _ (I.finIn π o t ho ht).1 _ (by simp [Ev.ids])

end Inv

theorem curOf_held {base π : Nat} {s : List (Frame T V)} (h : curOf base s = π) :
    (∃ fr ∈ s, fr.held = some π) ∨ base = π := by
  induction s with
  | nil => exact .inr h
  | cons fr s ih =>
    cases fr with
    | inside π' env => exact .inl ⟨_, List.mem_cons_self, by simpa [curOf, Frame.held] using h⟩
    | build πp m Ly σ Ls b => exact .inl ⟨_, List.mem_cons_self, by simpa [curOf, Frame.held] using h⟩
    | _ =>
      rcases ih h with ⟨fr', h₁, h₂⟩ | h'
      · exact .inl ⟨fr', List.mem_cons_of_mem _ h₁, h₂⟩
      · exact .inr h'

theorem step_fresh (I : Inv C) (hs : Step K L C C') :
    ∀ k, C'.n ≤ k → C'.Φ k = none ∧ C'.Λ k = none ∧ C'.G k = none ∧ C'.X k = none ∧
      C'.M k = none := by
  intro k hk
  have hk0 : C.n ≤ k := by cases hs <;> simp at hk <;> omega
  obtain ⟨h1, h2, h3, h4, h5⟩ := I.fresh k hk0
  cases hs <;> simp only [upd, setF, updO, setEnt] at hk ⊢ <;> grind

theorem ids_cons {e : Ev V} {J : List (Ev V)} {n : Nat} (h₁ : ∀ i ∈ e.ids, i < n)
    (h₂ : ∀ e ∈ J, ∀ i ∈ e.ids, i < n) : ∀ e' ∈ e :: J, ∀ i ∈ e'.ids, i < n := by
  intro e' he; rcases List.mem_cons.1 he with rfl | he
  · exact h₁
  · exact h₂ e' he

theorem ids_mono {J : List (Ev V)} {n n' : Nat} (h : ∀ e ∈ J, ∀ i ∈ e.ids, i < n)
    (hn : n ≤ n') : ∀ e ∈ J, ∀ i ∈ e.ids, i < n' :=
  fun e he i hi => Nat.lt_of_lt_of_le (h e he i hi) hn

set_option maxHeartbeats 2000000 in
theorem step_evIds (I : Inv C) (hs : Step K L C C') : ∀ e ∈ C'.J, ∀ i ∈ e.ids, i < C'.n := by
  have hJ := I.evIds
  have h1 : ∀ j (s : FState T V), C.Φ j = some s → j < C.n := fun _ _ h => I.ltΦ h
  have h2 : ∀ π (o : Owner T V), C.Λ π = some o → π < C.n := fun _ _ h => I.ltΛ h
  have h5 : ∀ m (mm : Memo V), C.M m = some mm → m < C.n := fun _ _ h => I.ltM h
  have h6 : ∀ j (φ : Fiber T V), C.Φ j = some (.run φ) → φ.cur < C.n := fun _ _ h =>
    let ⟨_, ho, _⟩ := I.cur_live h; I.ltΛ ho
  have h7 : ∀ j (φ : Fiber T V) fr, C.Φ j = some (.run φ) → fr ∈ φ.stack → FrOk C j fr :=
    fun j φ fr h hfr => I.frames j φ h fr hfr
  have h8 : ∀ π (o : Owner T V) i a fs, o.fs = ⟨i, a⟩ :: fs → C.Λ π = some o → i < C.n :=
    fun π o i a fs hfs ho => by
      have := (I.finIn π o ⟨i, a⟩ ho (by simp [hfs])).1
      exact hJ _ this i (by simp [Ev.ids])
  have h4 : ∀ p π, C.X p = some π → p < C.n := fun _ _ h => I.ltX h
  have hG : ∀ γ g, C.G γ = some g → g.isOpen = true → g.owner < C.n := fun γ g h ho =>
    let ⟨_, ho', _⟩ := (I.groups γ g h).1 ho; I.ltΛ ho'
  have hL : ∀ Ly fuel m m' (e : Ent V), lookup C.M Ly fuel m = some (m', e) → e.σ < C.n :=
    fun _ _ _ _ _ h => by
      obtain ⟨mm, hm, he⟩ := lookup_lt h
      obtain ⟨_, ⟨_, ho⟩, _⟩ := I.ents _ _ _ _ hm he
      exact I.ltΛ ho
  have hE : ∀ m (mm : Memo V) Ly (e : Ent V), C.M m = some mm → mm.ents Ly = some e →
      e.σ < C.n := fun _ _ _ _ hm he => by
    obtain ⟨_, ⟨_, ho⟩, _⟩ := I.ents _ _ _ _ hm he
    exact I.ltΛ ho
  have hMs : ∀ m, (C.M m).isSome → m < C.n := fun m h => by
    cases hm : C.M m with
    | none => simp [hm] at h
    | some mm => exact I.ltM hm
  have hB : ∀ j φ πp m Ly σ (Ls : List (Nat × T)) b s, C.Φ j = some (.run φ) →
      φ.stack = .build πp m Ly σ Ls b :: s → σ < C.n := fun j φ πp m Ly σ Ls b s h hs => by
    obtain ⟨_, ⟨_, ho, _⟩, _⟩ := (I.frames j φ h (.build πp m Ly σ Ls b) (by simp [hs])).build
    exact I.ltΛ ho
  cases hs <;> simp only <;>
    first
    | exact ids_mono hJ (by omega)
    | refine ids_cons ?_ (ids_mono hJ (by omega))
    | refine ids_cons ?_ (ids_cons ?_ (ids_mono hJ (by omega)))
    | refine ids_cons ?_ (ids_cons ?_ (ids_cons ?_ (ids_mono hJ (by omega))))
  all_goals
    simp only [Ev.ids, List.mem_cons, List.not_mem_nil, or_false, forall_eq_or_imp, forall_eq,
      List.forall_mem_cons]
    repeat' apply And.intro
  all_goals first
    | omega
    | exact Nat.lt_of_lt_of_le (h1 _ _ ‹_›) (by omega)
    | exact Nat.lt_of_lt_of_le (h2 _ _ ‹_›) (by omega)
    | exact Nat.lt_of_lt_of_le (h5 _ _ ‹_›) (by omega)
    | exact Nat.lt_of_lt_of_le (h6 _ _ ‹_›) (by omega)
    | exact fun _ h => h.elim
    | exact Nat.lt_of_lt_of_le (h8 _ _ _ _ _ ‹_› ‹_›) (by omega)
    | exact Nat.lt_of_lt_of_le (h4 _ _ ‹_›) (by omega)
    | exact Nat.lt_of_lt_of_le (hG _ _ ‹_› ‹_›) (by omega)
    | exact Nat.lt_of_lt_of_le (hL _ _ _ _ _ ‹_›) (by omega)
    | exact Nat.lt_of_lt_of_le (hE _ _ _ _ ‹_› ‹_›) (by omega)
    | exact Nat.lt_of_lt_of_le (hMs _ ‹_›) (by omega)
    | exact Nat.lt_of_lt_of_le (hB _ _ _ _ _ _ _ _ _ ‹_› ‹_›) (by omega)

end Flick.RT
