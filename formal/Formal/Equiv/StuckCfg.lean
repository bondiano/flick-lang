import Formal.Equiv.Stuck

/-!
# Equivalence of every run: a stuck configuration, run by run

`stuck_sim`: while the right configuration is not stuck, it steps, or its main thread dies, or it
is a literal. Each is answered (`thread_reflect`): the left configuration would step, or its main
thread die, or be the literal, none of which a stuck one does; so the right side catches up
alone, which lowers its potential (`rpot`). A thread that dies on the left is answered by one that
dies on the right (`die_sim`), which takes no other step (`Step.die_excl`).
-/

namespace Flick.Equiv

variable {W : World}

/-- What the right side still catches up alone. -/
def rpot (c : Cfg) : Nat := c.main.pot + ksum Tm.pot c.rgs

/-- The threads of a configuration are typed as terms. -/
def CfgTyped (S : Sig) (c : Cfg) : Prop :=
  TypedT S c.main ∧ ∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → ∀ k ∈ g.kids, TypedT S k

/-- A left thread related to a literal is that literal, or steps alone. -/
theorem ThR.lit_rev {μ : List Tm} {τ : Option (Nat × Nat)} {a : Tm} {o : Lit}
    (h : ThR W μ τ a (.lit o)) (hB : ¬ W.B) : a = .lit o ∨ LStep μ a := by
  rcases h.final_rev (.inl .lit) hB with hf | hs
  · rcases h with h | ⟨E, E', n, e, e', rfl, -, hE, -, -, -, hc, -⟩
    · rcases hf with hv | ⟨v, -, rfl⟩ | ⟨p, ℓ, c, rfl⟩
      · exact .inl (h.value_any hv .lit .top).rinv_lit
      · cases h
      · cases h
    · exact .inr ⟨_, _, E, _, _, hE, rfl, rfl, .inr fun _ => .forcePending hc⟩
  · exact .inr hs

/-- A right child that catches up alone, in its slot. -/
theorem kid_catch {root : Roots} {Λ Λ₂ : List Rgn} {μ μ₂ μ₂' : List Tm} {e e₂ k k₂ k₂' : Tm}
    {p j : Nat} {g g₂ : Rgn} {W' : World} (ht : ThR W μ none e e₂) (hst : StR W μ μ₂)
    (hrg : RgsR W μ none Λ Λ₂) (hg : Λ[p]? = some g) (hk : g.kids[j]? = some k)
    (hg₂ : Λ₂[p]? = some g₂) (hl₂ : g₂.live = true) (hk₂ : g₂.kids[j]? = some k₂)
    (hnf : ¬ Final k₂) (rj : RJ W μ₂ k₂ W' μ₂' k₂') (ht' : ThR W' μ (some (p, j)) k k₂') :
    CSteps root ⟨Λ₂, μ₂, e₂⟩ ⟨setKid Λ₂ p j k₂', μ₂', e₂⟩ ∧
      CfgR W' ⟨Λ, μ, e⟩ ⟨setKid Λ₂ p j k₂', μ₂', e₂⟩ ∧
      ksum Tm.pot (setKid Λ₂ p j k₂') + k₂.pot = ksum Tm.pot Λ₂ + k₂'.pot := by
  obtain ⟨g', hg', hR⟩ := hrg.get hg
  rw [hg₂] at hg'; cases hg'
  refine ⟨rj.ksteps.kid e₂ hg₂ hl₂ hk₂, ⟨ht.mono rj.le, rj.st _ hst, ?_⟩, ksum_set _ _ hg₂ hk₂⟩
  have := ((hrg.keep rj.le keepO_refl).any (some (p, j))).reslot ht' fun g' hg' hl _ hf => by
    rw [hg] at hg'; cases hg'
    exact (hnf (hR.fin j k k₂ (by simp) hl hk hk₂ hf)).elim
  rwa [Flick.Elab.setKid_same hg hk] at this

/-- **A stuck configuration** of the left side is answered by a right run to a stuck one. -/
theorem stuck_sim {S : Sig} {root : Roots} :
    ∀ (n : Nat) {W : World} {c₁ c₂ : Cfg}, rpot c₂ < n → CfgR W c₁ c₂ → ¬ W.B →
      CfgTyped S c₁ → Stuck root c₁ → ∃ c₂', CSteps root c₂ c₂' ∧ Stuck root c₂' := by
  intro n
  induction n with
  | zero => intro _ _ _ h; exact absurd h (Nat.not_lt_zero _)
  | succ n ih =>
    intro W c₁ c₂ hn hr hB hty hs
    obtain ⟨Λ, μ, e⟩ := c₁
    obtain ⟨Λ₂, μ₂, e₂⟩ := c₂
    obtain ⟨ht, hst, hrg⟩ := hr
    obtain ⟨hns, hnd, hnl⟩ := hs
    by_cases hs₂ : Stuck root ⟨Λ₂, μ₂, e₂⟩
    · exact ⟨_, .refl, hs₂⟩
    -- a step of the left side, which a stuck one does not take
    have moves : Moves root Λ μ e → False := by
      rintro (⟨l, o, h⟩ | ⟨p, g, hg, hl, k, hk, hs⟩)
      · cases o with
        | cfg => exact hns _ _ (.main h)
        | die => exact hnd ⟨_, h⟩
      · obtain ⟨j, hj⟩ := List.mem_iff_getElem?.mp hk
        obtain ⟨_, _, s⟩ := hs
        obtain ⟨_, s⟩ := s.step (root := root) Λ
        exact hns _ _ (.kid hg hl hj s)
    -- the right main thread catches up alone
    have main : ∀ {W' μ₂' t₂'}, RJ W μ₂ e₂ W' μ₂' t₂' → t₂'.pot < e₂.pot → ThR W' μ none e t₂' →
        ∃ c₂', CSteps root ⟨Λ₂, μ₂, e₂⟩ c₂' ∧ Stuck root c₂' := by
      intro W' μ₂' t₂' rj hlt ht'
      obtain ⟨c₂', hs', hs''⟩ := ih (c₂ := ⟨Λ₂, μ₂', t₂'⟩) (by simp only [rpot] at hn ⊢; omega)
        ⟨ht', rj.st _ hst, hrg.keep rj.le keepO_refl⟩ (fun h => hB (rj.le.B.2 h)) hty
        ⟨hns, hnd, hnl⟩
      exact ⟨c₂', (rj.ksteps.main Λ₂).trans hs', hs''⟩
    -- a step of the right main thread
    have mstep : ∀ {l o}, Step root l Λ₂ μ₂ e₂ o →
        ∃ c₂', CSteps root ⟨Λ₂, μ₂, e₂⟩ c₂' ∧ Stuck root c₂' := fun h => by
      rcases thread_reflect ht hst hrg hB hty.1 h with hm | ⟨W', μ₂', t₂', rj, hlt, ht'⟩
      · exact (moves hm).elim
      · exact main rj hlt ht'
    have hs₂' : (∃ l c, CStep root l ⟨Λ₂, μ₂, e₂⟩ c) ∨ MainDies root ⟨Λ₂, μ₂, e₂⟩ ∨
        ∃ o, e₂ = .lit o :=
      Classical.byContradiction fun hc => hs₂ ⟨fun l c h => hc (.inl ⟨l, c, h⟩),
        fun h => hc (.inr (.inl h)), fun o h => hc (.inr (.inr ⟨o, h⟩))⟩
    rcases hs₂' with ⟨l, c, hc⟩ | ⟨l, hd⟩ | ⟨o, ho⟩
    · cases hc with
      | main h => exact mstep h
      | @kid _ p g₂ j k₂ _ _ _ _ k₂' _ hg₂ hl₂ hk₂ h =>
        obtain ⟨g, hg, hR⟩ := hrg.get' hg₂
        obtain ⟨k, hk⟩ : ∃ k, g.kids[j]? = some k := by
          have : j < g.kids.length := hR.len ▸ Flick.Elab.lt_of_getElem? hk₂
          exact ⟨_, List.getElem?_eq_getElem this⟩
        have htk := hR.kids j k k₂ (by simp) hk hk₂
        have htyk := hty.2 p g hg k (List.mem_of_getElem? hk)
        have hnf : ¬ Final k₂ := h.not_final
        -- the right child catches up alone
        have kid : ∀ {W' μ₂' k₂'}, RJ W μ₂ k₂ W' μ₂' k₂' → k₂'.pot < k₂.pot →
            ThR W' μ (some (p, j)) k k₂' → ∃ c₂', CSteps root ⟨Λ₂, μ₂, e₂⟩ c₂' ∧ Stuck root c₂' := by
          intro W' μ₂' k₂' rj hlt ht'
          obtain ⟨hcs, hr', hk'⟩ := kid_catch ht hst hrg hg hk hg₂ hl₂ hk₂ hnf rj ht'
          dsimp only at hk'
          obtain ⟨c₂', hs', hs''⟩ := ih (c₂ := ⟨setKid Λ₂ p j k₂', μ₂', e₂⟩)
            (by simp only [rpot] at hn ⊢; omega) hr' (fun h => hB (rj.le.B.2 h)) hty ⟨hns, hnd, hnl⟩
          exact ⟨c₂', hcs.trans hs', hs''⟩
        rcases thread_reflect htk hst hrg hB htyk h with hm | ⟨W', μ₂', k₂', rj, hlt, ht'⟩
        · rcases hm with ⟨l', o, h'⟩ | hm
          · cases o with
            | cfg => exact (hns _ _ (.kid hg (hR.live ▸ hl₂) hk h')).elim
            | die =>
              obtain ⟨W', μ₂', k₂', l'', rj, hd, hs'⟩ := die_sim (Λ₂ := Λ₂) htk hst hB h'
              rcases rj.pot with ⟨rfl, rfl⟩ | hlt
              · exact (hd.die_excl h).elim
              · exact kid rj hlt (.inl hs')
          · exact (moves (.inr hm)).elim
        · exact kid rj hlt ht'
    · exact mstep hd
    · subst ho
      rcases ht.lit_rev hB with rfl | hs
      · exact (hnl _ rfl).elim
      · exact (moves hs.moves).elim

end Flick.Equiv
