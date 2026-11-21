import Formal.Equiv.RegionC

/-!
# Contextual equivalence: a step of a thread

**`thread_sim`**: a step of a thread of the left side is answered by the right side, by steps
that keep the regions and one more (`TRes`), or, for a head step that keeps the regions, by steps
that keep them, possibly none (`KAns`), with related results, stores and regions. What
either side reaches has no lazy cell where a pattern reads (`hzl`, `hzr`, which typing gives),
so neither takes a dead arm.
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

theorem sim_case {root : Roots} (hroot : root.Src) {l : Lbl} {Λ Λ' Λ₂ : List Rgn}
    {μ μ' μ₂ : List Tm} {τ : Option (Nat × Nat)} {t t₂ t' : Tm} (ht : Sim W .any t t₂)
    (hst : StR W μ μ₂) (hR : W.R = root) (hcl₁ : t.Closed) (hcl₁' : t'.Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed)
    (hmvl : t.MatchVals) (hmvr : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.MatchVals)
    (hcell : ∀ (l : Nat) (c : Tm), μ[l]? = some c → c.IsCell → ∃ c', μ'[l]? = some c' ∧ c'.IsCell)
    (hrg : RgsR W μ τ Λ Λ₂) (hpos : ThreadAt τ Λ t) (hs : Step root l Λ μ t (.cfg Λ' μ' t'))
    (hzl : ¬ t'.LzTop .any) (hzr : ∀ Λ' μ'' t'', TAns root Λ₂ μ₂ t₂ Λ' μ'' t'' → ¬ t''.LzTop .any) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ' μ' t' ∨ (Λ' = Λ ∧ KAns root W τ μ₂ t₂ μ μ' t t') := by
  have hzk : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → ¬ t''.LzTop .any :=
    fun _ _ hk => hzr _ _ _ ⟨_, _, hk, .inl ⟨rfl, rfl, rfl⟩⟩
  cases hs with
  | pure hE hh => exact .inr ⟨rfl, pure_sim hE hh ht hst hR hroot.2 hcl₁ hcl₁' hcl₂ hmvl hmvr hzl hzk⟩
  | rt hE hh =>
    rename_i e e'
    by_cases hreg : e.Reg
    · refine .inl ?_
      cases hh with
      | scope => exact scope_sim hE ht hst hR hcl₁ hcl₂ hrg
      | open_ hg hl => exact open_sim hE hg hl ht hst hR hcl₁ hcl₂ hrg
      | op hg hl => exact op_sim hE hg hl ht hst hR hcl₁ hcl₂ hrg
      | finRun hg hl hw hk hfs => exact finRun_sim hE hg hl hw hk hfs ht hst hR hcl₁ hcl₂ hrg hpos
      | close hg hl hw hk hfs => exact close_sim hE hg hl hw hk hfs ht hst hR hcl₁ hcl₂ hrg hpos
      | hdl => exact hdl_sim hE ht hst hR hcl₁ hcl₂ hrg
      | _ => simp [Tm.Reg] at hreg
    · exact .inr (rt_sim hE hh hreg ht hst hR hroot.2 hcl₁ hcl₁' hcl₂ hmvl hmvr hcell hzl hzk)
  | addFin hE hg hl hv => exact .inl <| addFin_sim hE hg hl hv ht hst hR hcl₁ hcl₂ hrg
  | spawn hE hg hl hv => exact .inl <| spawn_sim hE hg hl hv ht hst hR hcl₁ hcl₂ hrg
  | performRoot hE hh hr hf hv => exact .inl <| root_sim hroot hE hh hr hf hv ht hst hR hcl₁ hcl₂ hrg
  | raise hE hF hnt hni hnc hv => exact .inl <| raise_sim hE hF hnt hni hnc hv ht hst hR hcl₁ hcl₂ hrg
  | unw hE hF hni hnc hnh => exact .inl <| unw_sim hE hF hni hnc hnh ht hst hR hcl₁ hcl₂ hrg
  | performAbort hE hEi hh ha hv => exact .inl <| abort_sim hE hEi hh ha hv ht hst hR hcl₁ hcl₂ hrg hzl hzr
  | performTail hE hEi hh ha hv => exact .inl <| tail_sim hE hEi hh ha hv ht hst hR hcl₁ hcl₂ hrg hzl hzr
  | performKept hE hEi hh ha hv hq => exact .inl <| kept_sim hE hEi hh ha hv hq ht hst hR hcl₁ hcl₂ hrg hzl hzr
  | performCtl hE hEi hh ha hv => exact .inl <| ctl_sim hE hEi hh ha hv ht hst hR hcl₁ hcl₂ hrg hzl hzr
  | resume hE hA hEi hk hn hu =>
    subst hk
    exact .inl (resume_sim hE hA hEi hn hu ht hst hR hcl₁ hcl₂ hrg)
  | glob hE hg => exact .inr ⟨rfl, glob_sim hroot hE hg ht hst hR hcl₁ hcl₁' hcl₂⟩


/-- **A step of a thread** of the left side is answered by the right side. -/
theorem thread_sim {root : Roots} (hroot : root.Src) {l : Lbl} {Λ Λ' Λ₂ : List Rgn}
    {μ μ' μ₂ : List Tm} {τ : Option (Nat × Nat)} {t t₂ t' : Tm} (ht : ThR W μ τ t t₂)
    (hst : StR W μ μ₂) (hR : W.R = root) (hcl₁ : t.Closed) (hcl₁' : t'.Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed)
    (hmvl : t.MatchVals) (hmvr : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.MatchVals)
    (hcell : ∀ (l : Nat) (c : Tm), μ[l]? = some c → c.IsCell → ∃ c', μ'[l]? = some c' ∧ c'.IsCell)
    (hrg : RgsR W μ τ Λ Λ₂) (hpos : ThreadAt τ Λ t) (hs : Step root l Λ μ t (.cfg Λ' μ' t'))
    (hzl : ¬ t'.LzTop .any) (hzr : ∀ Λ' μ'' t'', TAns root Λ₂ μ₂ t₂ Λ' μ'' t'' → ¬ t''.LzTop .any) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ' μ' t' ∨ (Λ' = Λ ∧ KAns root W τ μ₂ t₂ μ μ' t t') := by
  rcases ht with ht | hos
  · exact sim_case hroot ht hst hR hcl₁ hcl₁' hcl₂ hmvl hmvr hcell hrg hpos hs hzl hzr
  · exact .inr (oneshot_sim hos hst hs)

/-- An answer of at least one step: steps that keep the regions, then one more, or at least one
that keeps them. -/
def TPos (root : Roots) (Λ₂ : List Rgn) (μ₂ : List Tm) (t₂ : Tm) (Λ₂' : List Rgn) (μ₂' : List Tm)
    (t₂' : Tm) : Prop :=
  (∃ μ₁ t₁ l, KSteps root μ₂ t₂ μ₁ t₁ ∧ Step root l Λ₂ μ₁ t₁ (.cfg Λ₂' μ₂' t₂')) ∨
    (Λ₂' = Λ₂ ∧ KPos root μ₂ t₂ μ₂' t₂')

/-- The answer to a step of a thread: steps that keep the regions and at most one more, at least
one or none (`Zero`). -/
theorem thread_ans {root : Roots} {Λ Λ' Λ₂ : List Rgn} {μ μ' μ₂ : List Tm}
    {τ : Option (Nat × Nat)} {t t₂ t' : Tm}
    (h : TRes root W τ Λ₂ μ₂ t₂ μ Λ' μ' t' ∨ (Λ' = Λ ∧ KAns root W τ μ₂ t₂ μ μ' t t'))
    (hrg : RgsR W μ τ Λ Λ₂) :
    ∃ W' Λ₂' μ₂' t₂', W.le W' ∧ TAns root Λ₂ μ₂ t₂ Λ₂' μ₂' t₂' ∧ ThR W' μ' τ t' t₂' ∧
      StR W' μ' μ₂' ∧ KeepO W τ μ μ' ∧ RgsR W' μ' τ Λ' Λ₂' ∧
      (TPos root Λ₂ μ₂ t₂ Λ₂' μ₂' t₂' ∨
        (Λ₂' = Λ₂ ∧ μ₂' = μ₂ ∧ t₂' = t₂ ∧ Λ' = Λ ∧ Zero W W' τ μ μ' t t')) := by
  rcases h with ⟨W', Λ₂', μ₂', t₂', hW, ⟨μ₁, t₁, l, hk, hs⟩, ht, hst, hko, hrg'⟩ |
    ⟨rfl, W', μ₂', t₂', hW, hk, ht, hst, hko⟩
  · exact ⟨W', Λ₂', μ₂', t₂', hW, ⟨μ₁, t₁, hk, .inr ⟨l, hs⟩⟩, ht, hst, hko, hrg',
      .inl (.inl ⟨μ₁, t₁, l, hk, hs⟩)⟩
  · refine ⟨W', Λ₂, μ₂', t₂', hW, ⟨_, _, KAns.ksteps hk, .inl ⟨rfl, rfl, rfl⟩⟩, ht, hst, hko,
      hrg.keep hW hko, ?_⟩
    rcases hk with hk | ⟨rfl, rfl, hz⟩
    · exact .inl (.inr ⟨rfl, hk⟩)
    · exact .inr ⟨rfl, rfl, rfl, rfl, hz⟩

end Flick.Equiv
