import Formal.Equiv.RegionC

/-!
# Contextual equivalence: a step of a thread

**`thread_sim`**: a step of a thread of the left side is answered by the right side, by steps
that keep the regions and at most one more, with related results, stores and regions. What
either side reaches has no lazy cell where a pattern reads (`hzl`, `hzr`, which typing gives),
so neither takes a dead arm.
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

theorem sim_case {root : Roots} (hroot : root.Src) {l : Lbl} {Λ Λ' Λ₂ : List Rgn}
    {μ μ' μ₂ : List Tm} {τ : Option (Nat × Nat)} {t t₂ t' : Tm} (ht : Sim W .any t t₂)
    (hst : StR W μ μ₂) (hcl₁ : t.Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂)
    (hpos : ThreadAt τ Λ t) (hs : Step root l Λ μ t (.cfg Λ' μ' t'))
    (hzl : ¬ t'.LzTop .any) (hzr : ∀ Λ' μ'' t'', TAns root Λ₂ μ₂ t₂ Λ' μ'' t'' → ¬ t''.LzTop .any) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ' μ' t' := by
  have hzk : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → ¬ t''.LzTop .any :=
    fun _ _ hk => hzr _ _ _ ⟨_, _, hk, .inl ⟨rfl, rfl, rfl⟩⟩
  cases hs with
  | pure hE hh => exact (pure_sim hE hh ht hst hcl₁ hcl₂ hzl hzk).res hrg
  | rt hE hh =>
    rename_i e e'
    by_cases hreg : e.Reg
    · cases hh with
      | scope => exact scope_sim hE ht hst hcl₁ hcl₂ hrg
      | open_ hg hl => exact open_sim hE hg hl ht hst hcl₁ hcl₂ hrg
      | op hg hl => exact op_sim hE hg hl ht hst hcl₁ hcl₂ hrg
      | finRun hg hl hw hk hfs => exact finRun_sim hE hg hl hw hk hfs ht hst hcl₁ hcl₂ hrg hpos
      | close hg hl hw hk hfs => exact close_sim hE hg hl hw hk hfs ht hst hcl₁ hcl₂ hrg hpos
      | hdl => exact hdl_sim hE ht hst hcl₁ hcl₂ hrg
      | _ => simp [Tm.Reg] at hreg
    · obtain ⟨rfl, hk⟩ := rt_sim hE hh hreg ht hst hcl₁ hcl₂ hzl hzk
      exact hk.res hrg
  | addFin hE hg hl hv => exact addFin_sim hE hg hl hv ht hst hcl₁ hcl₂ hrg
  | spawn hE hg hl hv => exact spawn_sim hE hg hl hv ht hst hcl₁ hcl₂ hrg
  | performRoot hE hh hr hf hv => exact root_sim hroot hE hh hr hf hv ht hst hcl₁ hcl₂ hrg
  | raise hE hF hnt hni hnc hv => exact raise_sim hE hF hnt hni hnc hv ht hst hcl₁ hcl₂ hrg
  | unw hE hF hni hnc hnh => exact unw_sim hE hF hni hnc hnh ht hst hcl₁ hcl₂ hrg
  | performAbort hE hEi hh ha hv => exact abort_sim hE hEi hh ha hv ht hst hcl₁ hcl₂ hrg hzl hzr
  | performTail hE hEi hh ha hv => exact tail_sim hE hEi hh ha hv ht hst hcl₁ hcl₂ hrg hzl hzr
  | performKept hE hEi hh ha hv hq => exact kept_sim hE hEi hh ha hv hq ht hst hcl₁ hcl₂ hrg hzl hzr
  | performCtl hE hEi hh ha hv => exact ctl_sim hE hEi hh ha hv ht hst hcl₁ hcl₂ hrg hzl hzr
  | resume hE hA hEi hk hn hu =>
    subst hk
    exact resume_sim hE hA hEi hn hu ht hst hcl₁ hcl₂ hrg
  | glob hE hg => exact glob_sim hroot hE hg ht hst hcl₁ hcl₂ hrg


/-- **A step of a thread** of the left side is answered by the right side. -/
theorem thread_sim {root : Roots} (hroot : root.Src) {l : Lbl} {Λ Λ' Λ₂ : List Rgn}
    {μ μ' μ₂ : List Tm} {τ : Option (Nat × Nat)} {t t₂ t' : Tm} (ht : ThR W μ τ t t₂)
    (hst : StR W μ μ₂) (hcl₁ : t.Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (hrg : RgsR W μ τ Λ Λ₂)
    (hpos : ThreadAt τ Λ t) (hs : Step root l Λ μ t (.cfg Λ' μ' t'))
    (hzl : ¬ t'.LzTop .any) (hzr : ∀ Λ' μ'' t'', TAns root Λ₂ μ₂ t₂ Λ' μ'' t'' → ¬ t''.LzTop .any) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ' μ' t' := by
  rcases ht with ht | hos
  · exact sim_case hroot ht hst hcl₁ hcl₂ hrg hpos hs hzl hzr
  · obtain ⟨rfl, hk⟩ := oneshot_sim hos hst hs
    exact hk.res hrg

end Flick.Equiv
