import Formal.Equiv.ThreadSim
import Formal.CfgTyped

/-!
# Contextual equivalence: the slot of a child

A child of a region steps in a configuration whose region holds it. A step of a term reads no
child of a region but in `finRun` and `close`, which need every child of the region finished; so
it is a step with any other unfinished term in the slot of an unfinished child (`Step.reslot`).
Steps that read no region lift to steps of the main thread and of a child (`KSteps.main`,
`KSteps.kid`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

/-- The region `q` with the child `j` of `p` replaced. -/
def Rgn.slot (q p j : Nat) (k : Tm) (g : Rgn) : Rgn :=
  if q = p then ⟨g.live, g.kids.set j k, g.fins⟩ else g

theorem setKid_getElem (Λ : List Rgn) (p j q : Nat) (k : Tm) :
    (setKid Λ p j k)[q]? = (Λ[q]?).map (Rgn.slot q p j k) := by
  unfold setKid
  split
  · next g hg =>
    by_cases hq : q = p
    · subst hq
      rw [setRgn, List.getElem?_set_self (Flick.Elab.lt_of_getElem? hg), hg]
      simp [Rgn.slot]
    · rw [setRgn, List.getElem?_set_ne (Ne.symm hq)]
      cases Λ[q]? <;> simp [Rgn.slot, hq]
  · next h =>
    by_cases hq : q = p
    · subst hq; simp [h]
    · cases Λ[q]? <;> simp [Rgn.slot, hq]

theorem setKid_setRgn {Λ : List Rgn} {p j q : Nat} {k : Tm} {g' : Rgn} :
    setKid (setRgn Λ q g') p j k = setRgn (setKid Λ p j k) q (Rgn.slot q p j k g') := by
  apply List.ext_getElem?
  intro i
  rw [setKid_getElem]
  simp only [setRgn, List.getElem?_set, setKid_length]
  rw [setKid_getElem]
  by_cases hi : q = i
  · subst hi
    split <;> simp
  · simp [hi]

theorem setKid_append {Λ : List Rgn} {p j : Nat} {k : Tm} {x : Rgn} (hp : p < Λ.length) :
    setKid (Λ ++ [x]) p j k = setKid Λ p j k ++ [x] := by
  apply List.ext_getElem?
  intro i
  rw [setKid_getElem]
  by_cases hi : i < Λ.length
  · rw [List.getElem?_append_left hi, List.getElem?_append_left (by rw [setKid_length]; exact hi),
      setKid_getElem]
  · rw [List.getElem?_append_right (by omega), List.getElem?_append_right (by rw [setKid_length]; omega),
      setKid_length]
    have : i ≠ p := by omega
    cases ([x])[i - Λ.length]? <;> simp [Rgn.slot, this]

theorem Live.slot {Λ : List Rgn} {q p j : Nat} {k : Tm} (h : Live Λ q) : Live (setKid Λ p j k) q := by
  obtain ⟨g, hg, hl⟩ := h
  refine ⟨Rgn.slot q p j k g, by rw [setKid_getElem, hg]; rfl, ?_⟩
  unfold Rgn.slot; split <;> exact hl

/-- A runtime head step that keeps the regions reads none. -/
theorem RHead.anyRgs {Λ Λ' : List Rgn} {μ μ' : List Tm} {e e' : Tm} (hh : RHead Λ μ e Λ' μ' e')
    (hreg : ¬ e.Reg) : ∀ Λ₀, RHead Λ₀ μ e Λ₀ μ' e' := by
  cases hh with
  | refMake h0 => exact fun _ => .refMake h0
  | refGet h0 h1 => exact fun _ => .refGet h0 h1
  | refGetCell h0 h1 => exact fun _ => .refGetCell h0 h1
  | refSet h0 h1 => exact fun _ => .refSet h0 h1
  | await => exact fun _ => .await
  | lazy => exact fun _ => .lazy
  | forcePending h0 => exact fun _ => .forcePending h0
  | forceDone h0 => exact fun _ => .forceDone h0
  | forceRunning h0 h1 h2 => exact fun _ => .forceRunning h0 h1 h2
  | settleVal h0 => exact fun _ => .settleVal h0
  | settleRaise h0 => exact fun _ => .settleRaise h0
  | settleUnw => exact fun _ => .settleUnw
  | tryRaise h0 => exact fun _ => .tryRaise h0
  | handleYes h0 h1 => exact fun _ => .handleYes h0 h1
  | handleNo h0 h1 => exact fun _ => .handleNo h0 h1
  | hguardYes h0 h1 => exact fun _ => .hguardYes h0 h1
  | hguardNo h0 h1 => exact fun _ => .hguardNo h0 h1
  | handleNil h0 => exact fun _ => .handleNil h0
  | scopeRun h0 => exact fun _ => .scopeRun h0
  | hctlRaise h0 h1 h2 => exact fun _ => .hctlRaise h0 h1 h2
  | hctlUnw h0 h1 => exact fun _ => .hctlUnw h0 h1
  | hresRaise h0 h1 h2 => exact fun _ => .hresRaise h0 h1 h2
  | hresUnw h0 h1 => exact fun _ => .hresUnw h0 h1
  | hkarmRaise h0 h1 h2 => exact fun _ => .hkarmRaise h0 h1 h2
  | hkarmUnw h0 h1 => exact fun _ => .hkarmUnw h0 h1
  | hwithCatch => exact fun _ => .hwithCatch
  | scope | open_ | op | finRun | close | hdl => exact (hreg trivial).elim

end Flick.Equiv

namespace Flick.Equiv

theorem Rgn.slot_live {q p j : Nat} {k : Tm} (g : Rgn) : (Rgn.slot q p j k g).live = g.live := by
  unfold Rgn.slot; split <;> rfl

theorem Rgn.slot_fins {q p j : Nat} {k : Tm} (g : Rgn) : (Rgn.slot q p j k g).fins = g.fins := by
  unfold Rgn.slot; split <;> rfl

theorem Rgn.slot_ne {q p j : Nat} {k : Tm} (g : Rgn) (h : q ≠ p) : Rgn.slot q p j k g = g := by
  simp [Rgn.slot, h]

/-- **A step with another unfinished term in the slot** of an unfinished child. -/
theorem _root_.Flick.Step.reslot {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' : List Tm} {t t' : Tm}
    (hs : Step root l Λ μ t (.cfg Λ' μ' t')) {p j : Nat} {g : Rgn} (hg : Λ[p]? = some g)
    {t₀ : Tm} (ht₀ : g.kids[j]? = some t₀) (hnf : ¬ Final t₀) (k : Tm) :
    Step root l (setKid Λ p j k) μ t (.cfg (setKid Λ' p j k) μ' t') := by
  have hp : p < Λ.length := Flick.Elab.lt_of_getElem? hg
  have get : ∀ {q g'}, Λ[q]? = some g' → (setKid Λ p j k)[q]? = some (Rgn.slot q p j k g') :=
    fun h => by rw [setKid_getElem, h]; rfl
  have nofin : ∀ {q g'}, Λ[q]? = some g' → (∀ k ∈ g'.kids, Final k) → q ≠ p := by
    rintro q g' h hk rfl
    rw [hg] at h; cases h
    exact hnf (hk _ (List.mem_of_getElem? ht₀))
  cases hs with
  | pure hE hh => exact .pure hE hh
  | rt hE hh =>
    rename_i e e'
    by_cases hreg : e.Reg
    · cases hh with
      | @scope _ _ e₀ =>
        rw [setKid_append hp]
        have := Step.rt (root := root) (Λ := setKid Λ p j k) (μ := μ) hE (RHead.scope (e := e₀))
        rwa [setKid_length] at this
      | @hdl _ _ Ef r e₀ al =>
        rw [setKid_append hp]
        have := Step.rt (root := root) (Λ := setKid Λ p j k) (μ := μ) hE
          (RHead.hdl (E := Ef) (al := al) (r := r) (e := e₀))
        rwa [setKid_length] at this
      | @open_ _ q g' _ hg' hl =>
        rw [setKid_setRgn]
        have := Step.rt (root := root) hE (RHead.open_ (μ := μ) (get hg') (by rw [Rgn.slot_live]; exact hl))
        unfold Rgn.slot at this ⊢
        split <;> simp_all
      | op hg' hl => exact .rt hE (.op (get hg') (by rw [Rgn.slot_live]; exact hl))
      | @finRun _ q g' w f _ fs hg' hl hw hk hfs =>
        have hq := nofin hg' hk
        rw [setKid_setRgn, Rgn.slot_ne _ hq]
        have := get hg'; rw [Rgn.slot_ne _ hq] at this
        exact .rt hE (.finRun this hl hw hk hfs)
      | @close _ q g' w _ hg' hl hw hk hfs =>
        have hq := nofin hg' hk
        rw [setKid_setRgn, Rgn.slot_ne _ hq]
        have := get hg'; rw [Rgn.slot_ne _ hq] at this
        exact .rt hE (.close this hl hw hk hfs)
      | _ => simp [Tm.Reg] at hreg
    · obtain rfl := hh.keep hreg
      exact .rt hE (RHead.anyRgs hh hreg _)
  | @addFin E Λ q g' t₁ _ hE hg' hl hv =>
    rw [setKid_setRgn]
    have := Step.addFin (root := root) (μ := μ) hE (get hg') (by rw [Rgn.slot_live]; exact hl) hv
    unfold Rgn.slot at this ⊢
    split <;> simp_all
  | @spawn E Λ q g' t₁ _ hE hg' hl hv =>
    rw [setKid_setRgn]
    have := Step.spawn (root := root) (μ := μ) hE (get hg') (by rw [Rgn.slot_live]; exact hl) hv
    unfold Rgn.slot at this ⊢
    split
    · rename_i hq; subst hq
      rw [hg] at hg'; cases hg'
      simp only at this ⊢
      rw [List.set_append_left _ _ (Flick.Elab.lt_of_getElem? ht₀)]
      exact this
    · simp_all
  | performRoot hE hh hr hf hv => exact .performRoot hE hh hr hf hv
  | raise hE hF hnt hni hnc hv => exact .raise hE hF hnt hni hnc hv
  | unw hE hF hni hnc hnh => exact .unw hE hF hni hnc hnh
  | performAbort hE hEi hh ha hv => exact .performAbort hE hEi hh ha hv
  | performTail hE hEi hh ha hv => exact .performTail hE hEi hh ha hv
  | performKept hE hEi hh ha hv hq => exact .performKept hE hEi hh ha hv (Live.slot hq)
  | performCtl hE hEi hh ha hv => exact .performCtl hE hEi hh ha hv
  | resume hE hA hEi hk hn hu => exact .resume hE hA hEi hk hn hu
  | glob hE hg' => exact .glob hE hg'

end Flick.Equiv

namespace Flick.Equiv

/-- A finished term in an ordered context around a non-value is the term itself. -/
theorem final_plug {E : List Frame} {x : Tm} (hx : ¬ Value x) (h : Final (plug E x)) :
    E = [] ∧ Final x := by
  rcases snoc_or E with rfl | ⟨E₀, G, rfl⟩
  · exact ⟨rfl, h⟩
  · exfalso
    rw [plug_snoc] at h
    rcases h with h | ⟨v, hv, h⟩ | ⟨_, _, _, h⟩
    · exact not_value_plugs hx (E₀ ++ [G]) (by rw [plug_snoc]; exact h)
    · cases G <;> simp only [Frame.plug] at h <;> cases h <;> exact not_value_plugs hx E₀ hv
    · cases G <;> simp only [Frame.plug] at h <;> cases h

theorem raise_frame_not_final {F : Frame} {v : Tm} : ¬ Final (F.plug (.raise v)) := fun h =>
  nomatch (final_plug (E := [F]) (fun h => by cases h) h).1

theorem unw_frame_not_final {F : Frame} {p ℓ : Nat} {a : Tm} : ¬ Final (F.plug (.unw p ℓ a)) :=
  fun h => nomatch (final_plug (E := [F]) (fun h => by cases h) h).1

/-- **A term that steps is not finished.** -/
theorem _root_.Flick.Step.not_final {root : Roots} {l : Lbl} {Λ : List Rgn} {μ : List Tm} {t : Tm} {o : Out}
    (hs : Step root l Λ μ t o) : ¬ Final t := by
  intro hf
  cases hs with
  | pure _ hh =>
    obtain ⟨-, hf | ⟨_, _, rfl⟩ | ⟨_, _, _, rfl⟩⟩ := final_plug hh.not_value hf
    · exact hh.not_value hf
    all_goals cases hh
  | rt _ hh =>
    obtain ⟨-, hf | ⟨_, _, rfl⟩ | ⟨_, _, _, rfl⟩⟩ := final_plug hh.not_value hf
    · exact hh.not_value hf
    all_goals cases hh
  | addFin =>
    obtain ⟨-, hf | ⟨_, _, h⟩ | ⟨_, _, _, h⟩⟩ := final_plug (fin_not_value (.inl rfl)) hf
    · exact fin_not_value (.inl rfl) hf
    all_goals cases h
  | spawn =>
    obtain ⟨-, hf | ⟨_, _, h⟩ | ⟨_, _, _, h⟩⟩ := final_plug (fin_not_value (.inr rfl)) hf
    · exact fin_not_value (.inr rfl) hf
    all_goals cases h
  | performRoot =>
    obtain ⟨-, hf | ⟨_, _, h⟩ | ⟨_, _, _, h⟩⟩ := final_plug perform_not_value hf
    · exact perform_not_value hf
    all_goals cases h
  | raise => exact raise_frame_not_final (final_plug (fun h => raise_frame_not_final (.inl h)) hf).2
  | unw => exact unw_frame_not_final (final_plug (fun h => unw_frame_not_final (.inl h)) hf).2
  | resumeDie =>
    obtain ⟨-, hf | ⟨_, _, h⟩ | ⟨_, _, _, h⟩⟩ := final_plug kont_not_value hf
    · exact kont_not_value hf
    all_goals cases h
  | todo =>
    obtain ⟨-, hf | ⟨_, _, h⟩ | ⟨_, _, _, h⟩⟩ := final_plug (fun h => by cases h) hf
    · cases hf
    all_goals cases h
  | glob =>
    obtain ⟨-, hf | ⟨_, _, h⟩ | ⟨_, _, _, h⟩⟩ := final_plug (fun h => by cases h) hf
    · cases hf
    all_goals cases h
  | _ =>
    obtain ⟨-, hf | ⟨_, _, h⟩ | ⟨_, _, _, h⟩⟩ := final_plug (fun h => by cases h) hf
    · cases hf
    all_goals cases h

end Flick.Equiv
