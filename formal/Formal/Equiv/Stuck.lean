import Formal.Equiv.Refl

/-!
# Equivalence of every run: a stuck configuration answered by a stuck one

**`thread_reflect`**: when a right thread steps, the related left thread steps, or a child of the
left side steps alone, or the right thread catches up alone, which lowers its potential
(`Tm.pot`). The left focus is found as for a step of the left side (`Sim.dec`): a special pair
(which steps), the frame of a left cell (which settles), or a congruence, where `reflect_focus`
answers.

**`stuck_sim`**: a stuck left configuration is answered by a right run to a stuck one. While the
right side can step, the left one cannot, so the right side only catches up, and its potential
(`rpot`) bounds how long; a thread that dies on the left is answered by one that dies on the
right (`die_sim`), which takes no other step (`Step.die_excl`); and a right main thread that is a
literal is related only to the same literal.
-/

namespace Flick.Equiv

open Flick.Elab (All2 kont_redex fin_redex perform_redex raise_redex unw_redex IsAlts decomp_exists)

variable {W : World}

/-- **A thread that steps to a defect** takes no other step. -/
theorem _root_.Flick.Step.die_excl {root : Roots} {l l' : Lbl} {Λ Λ' Λ'' : List Rgn} {μ μ' μ'' : List Tm}
    {t t' : Tm} (hd : Step root l Λ μ t .die) (hs : Step root l' Λ' μ' t (.cfg Λ'' μ'' t')) :
    False := by
  cases hd with
  | todo hE =>
    rcases Step.simple hE todo_redex trivial hs with ⟨_, hh, -⟩ | ⟨_, hh, -⟩ <;> cases hh
  | resumeDie hE hu hnr =>
    rename_i E u p ℓ
    have hr : Redex (.app (.kont p ℓ) u) := kont_redex hu
    have bad : ∀ {E' : List Frame} {r' : Tm}, OkE E' → Redex r' →
        plug E (.app (.kont p ℓ) u) = plug E' r' → r' = .app (.kont p ℓ) u :=
      fun hE' hr' he => (decomp_eq hr' hr hE' hE he.symm).2
    generalize ht : plug E (.app (.kont p ℓ) u) = t at hs
    cases hs with
    | pure hE' hh => cases bad hE' hh.redex ht; cases hh
    | rt hE' hh =>
      rcases hh.redex' with h' | ⟨F, v, hF, hv, rfl⟩ | ⟨F, q, ℓ, a, hF, rfl⟩
      · cases bad hE' h' ht; cases hh
      · cases bad (E' := F :: _) (hE'.cons' hF) (raise_redex hv) ht
      · cases bad (E' := F :: _) (hE'.cons' hF) (unw_redex _ _ _) ht
    | addFin hE' _ _ hv => cases bad hE' (fin_redex (.inl rfl) hv) ht
    | spawn hE' _ _ hv => cases bad hE' (fin_redex (.inr rfl) hv) ht
    | performRoot hE' _ _ _ hv => cases bad hE' (perform_redex hv) ht
    | raise hE' hF _ _ _ hv => cases bad (E' := _ :: _) (hE'.cons' hF) (raise_redex hv) ht
    | unw hE' hF => cases bad (E' := _ :: _) (hE'.cons' hF) (unw_redex _ _ _) ht
    | performAbort hE' hEi _ _ hv | performTail hE' hEi _ _ hv | performCtl hE' hEi _ _ hv =>
      cases bad (hEi.append' (hE'.cons' (F := .hwith _ _ _ _ _) trivial)) (perform_redex hv)
        (ht.trans (plug_append _ (.hwith _ _ _ _ _ :: _) _).symm)
    | performKept hE' hEi _ _ hv _ =>
      cases bad (hEi.append' (hE'.cons' (F := .hkeep _ _ _ _) trivial)) (perform_redex hv)
        (ht.trans (plug_append _ (.hkeep _ _ _ _ :: _) _).symm)
    | resume hE' hA hEi hk hn hu' =>
      obtain ⟨rfl, h⟩ := decomp_eq (kont_redex hu') hr
        (hA.append' (hE'.cons' (F := .hctlA _ _ _ _ _ _ _ _) trivial)) hE
        (ht.trans (plug_append _ (.hctlA _ _ _ _ _ _ _ _ :: _) _).symm).symm
      cases h
      exact hnr ⟨_, _, _, _, _, _, _, rfl, hn⟩
    | glob hE' _ => cases bad hE' (glob_redex _) ht

/-- The right side's answer when its thread steps and the left side does not. -/
def RCatch (W : World) (μ : List Tm) (τ : Option (Nat × Nat)) (μ₂ : List Tm) (t t₂ : Tm) :
    Prop :=
  ∃ W' μ₂' t₂', RJ W μ₂ t₂ W' μ₂' t₂' ∧ t₂'.pot < t₂.pot ∧ ThR W' μ τ t t₂'

theorem Catch.thr {μ μ₂ : List Tm} {τ : Option (Nat × Nat)} {t t₂ : Tm}
    (h : Catch W μ₂ t t₂) : RCatch W μ τ μ₂ t t₂ :=
  let ⟨W', μ₂', t₂', rj, hlt, hs⟩ := h; ⟨W', μ₂', t₂', rj, hlt, .inl hs⟩

/-- The left side of a special pair of O1a or O1c steps alone. -/
theorem Act.lstep {μ : List Tm} {a b : Tm} (h : Act W a b) (hB : ¬ W.B) : LStep μ a := by
  cases h with
  | dropA => exact ⟨_, _, [.let_ _], _, _, OkE.nil.cons' trivial, rfl, rfl, .inr fun _ => .lazy⟩
  | dropB => exact .pure (.let_ .loc)
  | forceA => exact ⟨_, _, [.force], _, _, OkE.nil.cons' trivial, rfl, rfl, .inr fun _ => .lazy⟩
  | bornA hB' | bornB hB' | bornS hB' | bornV hB' | dead hB'
  | o4B hB' | o4Alloc1 hB' | o4Bind hB' | o4Settle hB' => exact (hB hB').elim

theorem LStep.moves {root : Roots} {Λ : List Rgn} {μ : List Tm} {t : Tm} (h : LStep μ t) :
    Moves root Λ μ t :=
  let ⟨_, _, s⟩ := h; let ⟨_, s'⟩ := s.step (root := root) Λ; .inl ⟨_, _, s'⟩

/-- **A step of a right thread** is answered by the left side, or the right side catches up. -/
theorem thread_reflect {S : Sig} {root : Roots} {l : Lbl} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm}
    {τ : Option (Nat × Nat)} {t t₂ : Tm} {o₂ : Out} (ht : ThR W μ τ t t₂) (hst : StR W μ μ₂)
    (hrg : RgsR W μ none Λ Λ₂) (hB : ¬ W.B) (hty : TypedT S t)
    (hs₂ : Step root l Λ₂ μ₂ t₂ o₂) : Moves root Λ μ t ∨ RCatch W μ τ μ₂ t t₂ := by
  rcases ht with ht | ⟨E, E', n, e, e', rfl, rfl, hE, -, -, -, hc, -⟩
  case inr => exact .inl (.inl ⟨_, _, .rt hE (.forcePending hc)⟩)
  by_cases hf : Final t
  · obtain ⟨W', μ₂', b', rj, hb', s⟩ := ht.fcu rfl hf μ₂ (fun h => (hB h).elim)
    rcases rj.pot with ⟨-, rfl⟩ | hlt
    · exact (hs₂.not_final hb').elim
    · exact .inr ⟨W', μ₂', b', rj, hlt, .inl s⟩
  have hnv : ¬ Value t := fun hv => hf (.inl hv)
  obtain ⟨E, r, hE, hr, rfl⟩ := decomp_exists t hnv
  have hrc : ∀ l e a, r ≠ .rcons l e a := by
    rintro l e a rfl
    have he := hr.2 (.rconsL l a) e trivial rfl
    exact hr.1 (.rcons he (hr.2 (.rconsR l e) a he rfl))
  rcases ht.dec hr.1 (fun _ _ h => Redex.app hr h) hrc ((hty.inner hE hr.1 rfl).not_alts) _ _ rfl
      hE μ₂ (fun h => (hB h).elim) with
    ⟨W', μ', E₀, E₁, a, b, E₂, hj, rfl, rfl, -, -, hact⟩ | ⟨W', μ₂', E₂, r₂, hj, hfr, hE₂, hs⟩ |
    ⟨W', μ', E₂, n, a, b, hj, -, -, rfl, -, -⟩
  · rw [plug_append]
    obtain ⟨μ', a', s⟩ := hact.lstep (μ := μ) (fun h => hB (hj.le.B.2 h))
    exact .inl (LStep.moves ⟨μ', _, s.plug (fun G hG => hE G (List.mem_append_right _ hG))⟩)
  · have hrel : Sim W' .any (plug E r) (plug E₂ r₂) := hfr.plugs hs.lower
    rcases hj.pot with ⟨e₁, e₂⟩ | hlt
    · subst μ₂' t₂
      have hst' := hj.st _ hst
      have hrg' := hrg.keep hj.le keepO_refl
      have hB' : ¬ W'.B := fun h => hB (hj.le.B.2 h)
      rcases hs.redex_cu hr hB' μ₂ with hr₂ | ⟨W'', μ₂'', r₂', rj, hlt, hs'⟩
      · rcases reflect_focus hE hE₂ hr hr₂ hs hfr hst' hrg' hB' hty hs₂ with h | h
        · exact .inl h
        · obtain ⟨W'', μ₂'', t₂', rj, hlt, s⟩ := h
          exact .inr ⟨W'', μ₂'', t₂', hj.trans rj, hlt, .inl s⟩
      · exact .inr ⟨W'', μ₂'', plug E₂ r₂', hj.trans (rj.plug hE₂), pot_plug_lt E₂ hlt,
          .inl ((hfr.mono rj.le).plugs hs')⟩
    · exact .inr ⟨W', μ₂', _, hj, hlt, .inl hrel⟩
  · exact .inl (.inl ⟨_, _, .rt hE (.settleVal (hr.2 (.settle _) a trivial rfl))⟩)

end Flick.Equiv
