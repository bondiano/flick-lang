import Formal.RT.Reach

/-!
# λRT: R1–R4 (runtime.md §4)

Each property is read off `Inv`, so it holds in every reachable configuration (`Inv.reach`).
The journal is newest first: in `J = J₁ ++ e :: J₂`, `J₂` is what happened before `e`.
-/

namespace Flick.RT

variable {T V : Type} {C : Conf T V}

/-- **R1 (exactly once).** A finalizer handed to an owner — by `add_finalizer` or by an
`acquire_release` that returned — goes to `F(π)` or runs at once (Fin-Now); no finalizer
starts twice; when the closing of `π` ends (C6), every finalizer of `F(π)` has started. -/
def R1 (J : List (Ev V)) : Prop :=
  (∀ π i, .reg π i ∈ J → .fin π i ∈ J ∨ .now π i ∈ J) ∧
  (∀ J₁ π i J₂, J = J₁ ++ .run π i :: J₂ → ¬ Started i J₂) ∧
  (∀ J₁ π i J₂, J = J₁ ++ .now π i :: J₂ → ¬ Started i J₂) ∧
  (∀ J₁ π J₂, J = J₁ ++ .closed π :: J₂ → ∀ i, .fin π i ∈ J₂ → .run π i ∈ J₂)

/-- **R2 (order).** Finalizers start newest first (ids grow with registration), and the
first starts only after every fiber started in `π` — child, watcher, group's child,
branch — has ended and every Promise of `P(π)` has settled. -/
def R2 (J : List (Ev V)) : Prop :=
  (∀ J₁ π i J₂, J = J₁ ++ .fin π i :: J₂ → ∀ π' j, .fin π' j ∈ J₂ → j < i) ∧
  (∀ J₁ π i J₂, J = J₁ ++ .run π i :: J₂ → (∀ j, .run π j ∈ J₂ → i < j) ∧ KidsDone π J₂)

/-- **R3 (children do not outlive the owner).** Once the closing of `π` has ended, every
fiber started in `π` is finished, so it makes no step. -/
def R3 (C : Conf T V) : Prop :=
  ∀ π k, .closed π ∈ C.J → StartedIn π k C.J → ∃ x, C.Φ k = some (.done x)

/-- **R4 (a child's defect is not lost).** A child that failed with `c` and whose watcher
ended had its supervisor called with `c`; a supervisor's failure is in `D`. -/
def R4 (C : Conf T V) : Prop :=
  (∀ π k w c, .spawn π k w ∈ C.J → .done k (.fail c) ∈ C.J → DoneJ w C.J → .sup k c ∈ C.J) ∧
  ∀ c, .defect c ∈ C.J → c ∈ C.D

theorem Inv.r1 (I : Inv C) : R1 C.J :=
  ⟨I.regJ, fun J₁ _ _ J₂ h => (I.hist J₁ _ J₂ h).2.1, fun J₁ _ _ J₂ h => I.hist J₁ _ J₂ h,
   fun J₁ _ J₂ h => (I.hist J₁ _ J₂ h).2.2⟩

theorem Inv.r2 (I : Inv C) : R2 C.J :=
  ⟨fun J₁ _ _ J₂ h => (I.hist J₁ _ J₂ h).2.2.2,
   fun J₁ _ _ J₂ h => let g := I.hist J₁ _ J₂ h; ⟨g.2.2.1, g.2.2.2.2.2⟩⟩

/-- An owner whose closing event is in the journal exists. -/
theorem Inv.closed_owner (I : Inv C) {π : Nat} (hc : .closed π ∈ C.J) :
    ∃ o, C.Λ π = some o ∧ o.st = .closed := by
  obtain ⟨J₂, hg, hsub⟩ := hist_mem I.hist hc
  obtain ⟨J₃, hg', hsub'⟩ := hist_mem I.hist (hsub _ hg.1)
  obtain ⟨o, ho⟩ := I.closeΛ π (hsub' _ hg'.1)
  exact ⟨o, ho, (I.stJ π o ho).2.2.2 hc⟩

theorem Inv.r3 (I : Inv C) : R3 C := by
  intro π k hc hk
  obtain ⟨o, ho, hst⟩ := I.closed_owner hc
  obtain ⟨s, hs, hb⟩ := I.startJ π k hk
  cases s with
  | done x => exact ⟨x, hs⟩
  | run ψ =>
    obtain ⟨o', ho', hl⟩ := I.live k ψ hs
    rw [hb ψ rfl, ho] at ho'; cases ho'; rw [hst] at hl; cases hl

theorem Inv.r4 (I : Inv C) : R4 C := by
  refine ⟨fun π k w c hs hd ⟨x, hw⟩ => ?_, I.defectJ⟩
  rcases I.watchJ π k w hs with ⟨φ, g, hφ, _⟩ | ⟨_, h⟩
  · rw [(I.doneJ w x).1 hw] at hφ; cases hφ
  · exact h c hd

/-- R3 and R4 together: when the closing of `π` has ended, every failure of a child of `π`
has reached its supervisor. -/
theorem Inv.r4_closed (I : Inv C) {π k w : Nat} {c : Cause V} (hc : .closed π ∈ C.J)
    (hs : .spawn π k w ∈ C.J) (hd : .done k (.fail c) ∈ C.J) : .sup k c ∈ C.J := by
  obtain ⟨x, hx⟩ := I.r3 π w hc (.inr (.inl ⟨k, hs⟩))
  exact I.r4.1 π k w c hs hd ⟨x, (I.doneJ w x).2 hx⟩

section Par

variable [DecidableEq V]

theorem mem_comb {a b : Cause V} {r : Reason V} (h : r ∈ a ∨ r ∈ b) : r ∈ comb a b := by
  unfold comb
  split
  · next ha => subst ha; simpa using h
  · split
    · next hb => subst hb; simpa using h
    · exact mem_nub.2 (List.mem_append.2 h)

/-- R4 for `parallel` (Par): with G7(f), the result keeps every reason of the first failure
and every reason but `Int` of the branches that failed after it. -/
theorem parResult_keeps {L : Lib V} (h : Hyp L) (unit : V) (xs : List (Out V)) {c : Cause V}
    (hc : Out.fail c ∈ xs) {r : Reason V} (hr : r ∈ noInt c) :
    ∃ c', parResult L unit xs = .fail c' ∧ r ∈ c' := by
  unfold parResult
  have hmem : c ∈ failsOf xs := List.mem_filterMap.2 ⟨_, hc, rfl⟩
  generalize failsOf xs = cs at hmem
  cases cs with
  | nil => cases hmem
  | cons c₁ cs =>
    refine ⟨_, rfl, ?_⟩
    have key : ∀ (a : Cause V) (l : List (Cause V)), (r ∈ a ∨ ∃ c ∈ l, r ∈ noInt c) →
        r ∈ l.foldl (fun a c' => L.combine a (noInt c')) a := by
      intro a l
      induction l generalizing a with
      | nil => rintro (h' | ⟨_, h', _⟩); exact h'; cases h'
      | cons d l ih =>
        rintro (h' | ⟨c', hc', hr'⟩)
        · exact ih (L.combine a (noInt d)) (.inl (by rw [h.g7e_combine]; exact mem_comb (.inl h')))
        · rcases List.mem_cons.1 hc' with rfl | hc'
          · exact ih (L.combine a (noInt c')) (.inl (by rw [h.g7e_combine]; exact mem_comb (.inr hr')))
          · exact ih _ (.inr ⟨c', hc', hr'⟩)
    rcases List.mem_cons.1 hmem with rfl | hmem
    · exact key _ _ (.inl (List.mem_filter.1 hr).1)
    · exact key _ _ (.inr ⟨c, hmem, hr⟩)

end Par

end Flick.RT
