import Formal.RT.Pres1

/-!
# λRT: who waits for a running fiber (the sketch of R3)

When the closing of `π` reaches (C5), no running fiber started in `π`: a child or a watcher
would be in `K(π)` or `W(π)`, a group's child would keep its group's hook in `H(π)`, and a
branch would keep its combinator running, which by induction did not start in `π` either.
-/

namespace Flick.RT

variable {T V : Type} {C : Conf T V}

theorem frame_held_open (I : Inv C) {j : Nat} {ψ : Fiber T V} {fr : Frame T V} {π : Nat}
    (hj : C.Φ j = some (.run ψ)) (hfr : fr ∈ ψ.stack) (hh : fr.held = some π) :
    ∃ o, C.Λ π = some o ∧ o.st = .open := by
  have := I.frames j ψ hj fr hfr
  cases fr with
  | inside π' env =>
    simp only [Frame.held, Option.some.injEq] at hh; subst hh
    obtain ⟨_, o, ho, hst, _⟩ := this.inside; exact ⟨o, ho, hst⟩
  | build πp m Ly σ Ls b =>
    simp only [Frame.held, Option.some.injEq] at hh; subst hh
    obtain ⟨_, ⟨o, ho, hst, _⟩, _⟩ := this.build; exact ⟨o, ho, hst⟩
  | _ => simp [Frame.held] at hh

theorem ParHas.sub {j π base : Nat} {s : List (Frame T V)} (h : ParHas j π base s) :
    ∃ b, (∀ fr ∈ b, fr ∈ s) ∧ curOf base b = π := by
  induction s with
  | nil => cases h
  | cons fr s ih =>
    have weaken : (∃ b, (∀ fr ∈ b, fr ∈ s) ∧ curOf base b = π) →
        ∃ b, (∀ fr' ∈ b, fr' ∈ fr :: s) ∧ curOf base b = π :=
      fun ⟨b, h1, h2⟩ => ⟨b, fun x hx => List.mem_cons_of_mem _ (h1 x hx), h2⟩
    cases fr with
    | par ks =>
      rcases h with ⟨_, h⟩ | h
      · exact ⟨s, fun x hx => List.mem_cons_of_mem _ hx, h⟩
      · exact weaken (ih h)
    | _ => exact weaken (ih h)

theorem ParHas.cons {j π base : Nat} {s : List (Frame T V)} (fr : Frame T V)
    (h : ParHas j π base s) : ParHas j π base (fr :: s) := by
  cases fr <;> first | exact .inr h | exact h

theorem ParHas.append {j π base : Nat} {s : List (Frame T V)} (X : List (Frame T V))
    (h : ParHas j π base s) : ParHas j π base (X ++ s) := by
  induction X with
  | nil => exact h
  | cons fr X ih => exact ih.cons fr

theorem ParHas.pop {j π base : Nat} {s : List (Frame T V)} {fr : Frame T V}
    (hfr : ∀ ks, fr ≠ .par ks) (h : ParHas j π base (fr :: s)) : ParHas j π base s := by
  cases fr with
  | par ks => exact absurd rfl (hfr ks)
  | _ => exact h

/-- A local step keeps the branches its `par` frames list, unless they are done. -/
theorem Local.parHas {K : Core T V} {L : Lib V} {φ φ' : Fiber T V} {j π : Nat}
    (hw : L.joinWaits = true) (h : Local K L C φ φ') (hj : ¬ Done C.Φ j)
    (hp : ParHas j π φ.base φ.stack) : ParHas j π φ'.base φ'.stack := by
  cases h with
  | resume _ hs | then_ _ hs | acqFail _ hs =>
    rw [hs] at hp; exact hp.pop (fun _ h => by cases h)
  | acquire => exact (hp.cons _).cons _
  | join | modRead => exact hp.cons _
  | par => exact (hp.cons _).cons _
  | parDone _ hs hd =>
    rw [hs] at hp
    rcases hp with ⟨hk, _⟩ | hp
    · exact absurd (hd j hk) hj
    · exact hp
  | parEarly hw' => rw [hw] at hw'; cases hw'
  | _ => exact hp

/-- No running fiber started in an owner whose closing is ready for (C5). -/
theorem Inv.noRun (I : Inv C) {π : Nat} {o : Owner T V} (hπ : π ≠ 0) (ho : C.Λ π = some o)
    (hst : o.st = .closing) (hph : o.ph = .hooks true) (hH : o.H = []) (hK : o.K = [])
    (hW : o.W = []) : ∀ j ψ, C.Φ j = some (.run ψ) → ψ.base ≠ π := by
  intro j
  induction j using Nat.strongRecOn with
  | _ j ih =>
  intro ψ hj hb
  obtain ⟨gm, gc, gw, gg, gb⟩ := I.guard j ψ hj
  cases hr : ψ.role with
  | main => exact hπ (hb ▸ gm hr)
  | child => obtain ⟨o', ho', hk⟩ := gc hr; rw [hb, ho] at ho'; cases ho'; simp [hK] at hk
  | watcher => obtain ⟨o', ho', hk⟩ := gw hr; rw [hb, ho] at ho'; cases ho'; simp [hW] at hk
  | gkid γ =>
    obtain ⟨g, hgγ, hown, hk⟩ := gg γ hr
    obtain ⟨h1, h2⟩ := I.groups γ g hgγ
    cases hop : g.isOpen
    · rcases h2 hop with hd | ⟨o', b, ho', _, hph'⟩
      · obtain ⟨x, hx⟩ := hd j hk; rw [hj] at hx; cases hx
      · rw [hown, hb, ho] at ho'; cases ho'; rw [hph] at hph'; cases hph'
    · obtain ⟨o', ho', _, hs⟩ := h1 hop
      rw [hown, hb, ho] at ho'; cases ho'; simp [hH] at hs
  | branch f' =>
    obtain ⟨hlt, ψ', hf', hpar⟩ := gb f' hr
    obtain ⟨b, hsub, hcur⟩ := hpar.sub
    rcases curOf_held (hcur.trans hb) with ⟨fr, hfr, hh⟩ | hb'
    · obtain ⟨o', ho', hopen⟩ := frame_held_open I hf' (hsub fr hfr) hh
      rw [ho] at ho'; cases ho'; rw [hst] at hopen; cases hopen
    · exact ih f' hlt ψ' hf' hb'

end Flick.RT
