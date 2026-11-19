import Formal.Equiv.Thread

/-!
# Contextual equivalence: steps at a known redex

A thread whose redex is an allocation, a `let`, a force, a settle or `todo` steps only by the
head step of that redex (`Step.simple`): those redexes need no context.
-/

namespace Flick.Equiv

open Flick.Elab (raise_redex unw_redex perform_redex kont_redex fin_redex)

/-- Redexes whose steps are head steps. -/
def _root_.Flick.Tm.Simple : Tm → Prop
  | .lazy _ | .let_ _ _ | .force _ | .settle _ _ | .todo => True
  | _ => False

theorem Step.simple {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' : List Tm} {E : List Frame}
    {r t' : Tm} (hE : OkE E) (hr : Redex r) (hs : r.Simple)
    (h : Step root l Λ μ (plug E r) (.cfg Λ' μ' t')) :
    (∃ r', PHead r r' ∧ Λ' = Λ ∧ μ' = μ ∧ t' = plug E r') ∨
      (∃ r', RHead Λ μ r Λ' μ' r' ∧ t' = plug E r') := by
  have bad : ∀ {E' : List Frame} {r' : Tm}, OkE E' → Redex r' → ¬ r'.Simple →
      plug E' r' = plug E r → False := fun hE' hr' hn he => by
    obtain ⟨-, rfl⟩ := decomp_eq hr' hr hE' hE he
    exact hn hs
  generalize ht : plug E r = t at h
  cases h with
  | pure hE' hh =>
    obtain ⟨rfl, rfl⟩ := decomp_eq hh.redex hr hE' hE ht.symm
    exact .inl ⟨_, hh, rfl, rfl, rfl⟩
  | rt hE' hh =>
    rcases hh.redex' with h' | ⟨F, v, hF, hv, rfl⟩ | ⟨F, q, ℓ, a, hF, rfl⟩
    · obtain ⟨rfl, rfl⟩ := decomp_eq h' hr hE' hE ht.symm
      exact .inr ⟨_, hh, rfl⟩
    · exact (bad (E' := F :: _) (hE'.cons' hF) (raise_redex hv) (by simp [Tm.Simple]) ht.symm).elim
    · exact (bad (E' := F :: _) (hE'.cons' hF) (unw_redex _ _ _) (by simp [Tm.Simple]) ht.symm).elim
  | addFin hE' _ _ hv =>
    exact (bad hE' (fin_redex (.inl rfl) hv) (by simp [Tm.Simple]) ht.symm).elim
  | spawn hE' _ _ hv =>
    exact (bad hE' (fin_redex (.inr rfl) hv) (by simp [Tm.Simple]) ht.symm).elim
  | performRoot hE' _ _ _ hv =>
    exact (bad hE' (perform_redex hv) (by simp [Tm.Simple]) ht.symm).elim
  | raise hE' hF _ _ _ hv =>
    exact (bad (E' := _ :: _) (hE'.cons' hF) (raise_redex hv) (by simp [Tm.Simple]) ht.symm).elim
  | unw hE' hF =>
    exact (bad (E' := _ :: _) (hE'.cons' hF) (unw_redex _ _ _) (by simp [Tm.Simple]) ht.symm).elim
  | performAbort hE' hEi _ _ hv =>
    exact (bad (hEi.append' (hE'.cons' (F := .hwith _ _ _ _ _) trivial)) (perform_redex hv)
      (by simp [Tm.Simple]) ((plug_append _ _ _).trans ht.symm)).elim
  | performTail hE' hEi _ _ hv =>
    exact (bad (hEi.append' (hE'.cons' (F := .hwith _ _ _ _ _) trivial)) (perform_redex hv)
      (by simp [Tm.Simple]) ((plug_append _ _ _).trans ht.symm)).elim
  | performCtl hE' hEi _ _ hv =>
    exact (bad (hEi.append' (hE'.cons' (F := .hwith _ _ _ _ _) trivial)) (perform_redex hv)
      (by simp [Tm.Simple]) ((plug_append _ _ _).trans ht.symm)).elim
  | performKept hE' hEi _ _ hv _ =>
    exact (bad (hEi.append' (hE'.cons' (F := .hkeep _ _ _ _) trivial)) (perform_redex hv)
      (by simp [Tm.Simple]) ((plug_append _ _ _).trans ht.symm)).elim
  | resume hE' hA hEi hk _ hu =>
    exact (bad (hA.append' (hE'.cons' (F := .hctlA _ _ _ _ _ _ _ _) trivial)) (kont_redex hu)
      (by simp [Tm.Simple]) ((plug_append _ _ _).trans ht.symm)).elim
  | glob hE' _ =>
    exact (bad hE' (glob_redex _) (by simp [Tm.Simple]) ht.symm).elim

theorem Frame.free_of {F : Frame} {x : Tm} {i : Nat} (h : x.Free i) : (F.plug x).Free i := by
  cases F <;> simp only [Frame.plug, Tm.Free] <;> simp [h]

theorem plug_closed {x : Tm} : ∀ {E : List Frame}, (plug E x).Closed → x.Closed
  | [], h => h
  | _ :: E, h => fun i hi => plug_closed (E := E) h i (Frame.free_of hi)

end Flick.Equiv
