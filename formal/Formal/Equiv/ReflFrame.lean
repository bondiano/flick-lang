import Formal.Equiv.ReflHead

/-!
# Equivalence of every run: a raised error or a signal of the right side leaves a frame

The right side takes a head step at a raised error or a signal in the frame around it. The left
side does too (`raise_frame_rev`, `unw_frame_rev`): the frame is a related one, or the frame of a
left cell, which the left side settles; or it is the frame of a right cell, and the right side has
caught up alone (`Catch`). The arm of a handler holds a parked body (`TypedT`).
-/

namespace Flick.Equiv

open Flick.Elab (All2 ORel)

variable {W : World}

/-- What the left side does when the right side steps: the thread steps, or one of the children
of a live region steps alone. -/
def Moves (root : Roots) (Λ : List Rgn) (μ : List Tm) (t : Tm) : Prop :=
  (∃ l o, Step root l Λ μ t o) ∨
    ∃ (p : Nat) (g : Rgn), Λ[p]? = some g ∧ g.live = true ∧ ∃ k ∈ g.kids, LStep μ k

/-- The right side catches up alone, the left thread as it is. -/
def Catch (W : World) (μ₂ : List Tm) (t t₂ : Tm) : Prop :=
  ∃ W' μ₂' t₂', RJ W μ₂ t₂ W' μ₂' t₂' ∧ t₂'.pot < t₂.pot ∧ Sim W' .any t t₂'

theorem HMoves.moves {root : Roots} {Λ : List Rgn} {μ : List Tm} {E : List Frame} {r : Tm}
    (hE : OkE E) (h : HMoves Λ μ r) : Moves root Λ μ (plug E r) := by
  rcases h with ⟨Λ', μ', r', h⟩ | h
  · exact .inl ⟨_, _, .rt hE h⟩
  · exact .inr h

/-- The frame of a right cell around a raised error or a signal, settled alone. -/
theorem catch_settle {μ₂ μ₂' : List Tm} {E E₂ : List Frame} {n : Nat} {r a₂ a₂' : Tm}
    (hfr : FrL W E E₂) (hs : Sim W .any r a₂') (hE₂ : OkE E₂)
    (hj : RJ W μ₂ (.settle n a₂) W μ₂' a₂') (hlt : a₂'.pot < (Tm.settle n a₂).pot) :
    Catch W μ₂ (plug E r) (plug (.settle n :: E₂) a₂) :=
  ⟨W, μ₂', plug E₂ a₂', hj.plug hE₂, pot_plug_lt E₂ hlt, hfr.plugs hs⟩

/-- A region the right side closes or runs a finalizer of: so does the left side, or one of its
children steps alone. -/
theorem inside_rev {root : Roots} {Λ Λ₂ : List Rgn} {μ : List Tm} {E : List Frame} {p : Nat}
    {w : Tm} {g₂ : Rgn} (hE : OkE E) (hw : Final w) (hrg : RgsR W μ none Λ Λ₂) (hB : ¬ W.B)
    (hg : Λ₂[p]? = some g₂) (hl : g₂.live = true) (hk : ∀ k ∈ g₂.kids, Final k)
    (hfs : g₂.fins = [] ∨ ∃ fs f, g₂.fins = fs ++ [f]) : Moves root Λ μ (plug E (.inside p w)) := by
  obtain ⟨g, hg', hR⟩ := hrg.get' hg
  have hl' : g.live = true := hR.live ▸ hl
  rcases hR.kids_rev hk hB with hk' | hs'
  · rcases hfs with hfs | ⟨fs, f, hfs⟩
    · exact .inl ⟨_, _, .rt hE (.close hg' hl' hw hk' (All2.nil_right (hfs ▸ hR.fins)))⟩
    · obtain ⟨fs', f', hfs'⟩ := All2.snoc_left (hfs ▸ hR.fins)
      exact .inl ⟨_, _, .rt hE (.finRun hg' hl' hw hk' hfs')⟩
  · exact .inr ⟨_, g, hg', hl', hs'⟩

/-- **A raised error leaves a frame** by a head step of the right side. -/
theorem raise_frame_rev {S : Sig} {root : Roots} {Λ Λ₂ Λ₂' : List Rgn} {μ μ₂ μ₂' : List Tm}
    {E E₂ : List Frame} {F : Frame} {r v₂ x' : Tm} (hE : OkE E) (hE₂ : OkE E₂) (hr : Redex r)
    (hs : Sim W .top r (.raise v₂)) (hv₂ : Value v₂) (hfr : FrL W E (F :: E₂))
    (hrg : RgsR W μ none Λ Λ₂) (hB : ¬ W.B) (hty : TypedT S (plug E r))
    (hh : RHead Λ₂ μ₂ (F.plug (.raise v₂)) Λ₂' μ₂' x') :
    Moves root Λ μ (plug E r) ∨ Catch W μ₂ (plug E r) (plug (F :: E₂) (.raise v₂)) := by
  obtain ⟨e, rfl, he⟩ := hs.rinv_raise
  have hev := hr.2 .raise e trivial rfl
  cases hfr with
  | @junkL n E₀ _ hn _ =>
    exact .inl (.inl ⟨_, _, .rt (E := E₀) (hE.cons).2 (.settleRaise hev)⟩)
  | junkR hn hfr' =>
    exact .inr (catch_settle hfr' (.raise he) hE₂ (RJ.settleR hn hv₂) (by simp [Tm.pot]))
  | @cons F' _ E' _ hFS hfr' =>
    have hE' := (hE.cons).2
    have hty' : TypedT S (F'.plug (.raise e)) :=
      hty.inner hE' (not_value_plug raise_not_value F') rfl
    have parked := hty'
    obtain ⟨_, _, _, _, ht⟩ := parked
    cases hFS <;> simp only [Frame.plug] at hh ht <;> cases hh
    all_goals first
      | (exfalso; exact raise_not_value ‹_›)
      | exact .inl (.inl ⟨_, _, .rt (E := E') hE' (.tryRaise hev)⟩)
      | exact .inl (.inl ⟨_, _, .rt (E := E') hE' (.settleRaise hev)⟩)
      | (obtain ⟨Ei, hEi, rfl⟩ := ht.hctl_parked rfl
         exact .inl (.inl ⟨_, _, .rt (E := E') hE' (.hctlRaise hEi rfl hev)⟩))
      | (obtain ⟨A, hA, rfl⟩ := ht.hres_parked rfl
         exact .inl (.inl ⟨_, _, .rt (E := E') hE' (.hresRaise hA rfl hev)⟩))
      | (obtain ⟨Ei, hEi, rfl⟩ := ht.hkarm_parked rfl
         exact .inl (.inl ⟨_, _, .rt (E := E') hE' (.hkarmRaise hEi rfl hev)⟩))
      | exact .inl (inside_rev hE' (.inr (.inl ⟨_, hev, rfl⟩)) hrg hB ‹_› ‹_› ‹_›
          (.inr ⟨_, _, ‹_›⟩))
      | exact .inl (inside_rev hE' (.inr (.inl ⟨_, hev, rfl⟩)) hrg hB ‹_› ‹_› ‹_› (.inl ‹_›))

/-- **A signal leaves a frame** by a head step of the right side. -/
theorem unw_frame_rev {S : Sig} {root : Roots} {Λ Λ₂ Λ₂' : List Rgn} {μ μ₂ μ₂' : List Tm}
    {E E₂ : List Frame} {F : Frame} {r : Tm} {q ℓ' : Nat} {c₂ x' : Tm} (hE : OkE E) (hE₂ : OkE E₂)
    (hs : Sim W .top r (.unw q ℓ' c₂)) (hfr : FrL W E (F :: E₂)) (hst : StR W μ μ₂)
    (hrg : RgsR W μ none Λ Λ₂) (hB : ¬ W.B) (hty : TypedT S (plug E r))
    (hh : RHead Λ₂ μ₂ (F.plug (.unw q ℓ' c₂)) Λ₂' μ₂' x') :
    Moves root Λ μ (plug E r) ∨ Catch W μ₂ (plug E r) (plug (F :: E₂) (.unw q ℓ' c₂)) := by
  obtain ⟨ℓ, k, c, rfl, hl, hc⟩ := hs.rinv_unw
  have hw : Final (Tm.unw q ℓ c) := .inr (.inr ⟨_, _, _, rfl⟩)
  cases hfr with
  | @junkL n E₀ _ hn _ =>
    exact .inl (.inl ⟨_, _, .rt (E := E₀) (hE.cons).2 .settleUnw⟩)
  | junkR hn hfr' =>
    exact .inr (catch_settle hfr' (.unw hl hc) hE₂ (RJ.settleU hn) (by simp [Tm.pot]))
  | @cons F' _ E' _ hFS hfr' =>
    have hE' := (hE.cons).2
    have hty' : TypedT S (F'.plug (.unw q ℓ c)) :=
      hty.inner hE' (not_value_plug (fun h => by cases h) F') rfl
    obtain ⟨_, _, _, _, ht⟩ := hty'
    cases hFS <;> simp only [Frame.plug] at hh ht <;> cases hh
    all_goals first
      | (exfalso; cases ‹Value (Tm.unw _ _ _)›)
      | exact .inl (.inl ⟨_, _, .rt (E := E') hE' .settleUnw⟩)
      | (obtain ⟨Ei, hEi, rfl⟩ := ht.hctl_parked rfl
         exact .inl (.inl ⟨_, _, .rt (E := E') hE' (.hctlUnw hEi rfl)⟩))
      | (obtain ⟨A, hA, rfl⟩ := ht.hres_parked rfl
         exact .inl (.inl ⟨_, _, .rt (E := E') hE' (.hresUnw hA rfl)⟩))
      | (obtain ⟨Ei, hEi, rfl⟩ := ht.hkarm_parked rfl
         exact .inl (.inl ⟨_, _, .rt (E := E') hE' (.hkarmUnw hEi rfl)⟩))
      | (obtain rfl := (hst.bij _ _ _ _ _ _ ‹W.L _ ℓ' _› hl).2 rfl
         exact .inl (.inl ⟨_, _, .rt (E := E') hE' .hwithCatch⟩))
      | exact .inl (inside_rev hE' hw hrg hB ‹_› ‹_› ‹_› (.inr ⟨_, _, ‹_›⟩))
      | exact .inl (inside_rev hE' hw hrg hB ‹_› ‹_› ‹_› (.inl ‹_›))

end Flick.Equiv
