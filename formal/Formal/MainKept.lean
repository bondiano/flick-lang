import Formal.CfgTyped

/-!
# Only a child keeps a handler

A kept handler (`hkeep`, and `hkarm` while its arm runs) is a frame of a child or a finalizer:
`spawn` and `addFin` wrap the work in the handlers of their place (`withs`) and register it with
a region, which runs it as a child; no step moves a child into the main thread, and a step of
the main thread builds a kept handler only from one it holds. A program has none, so neither
has its main thread after any steps (`Site.kept`, `steps_kept` in `Soundness`).
-/

namespace Flick

variable {S : Sig}

/-- A kept handler is a run-time form of a region. -/
theorem Tm.kept_ins {e : Tm} (h : e.Has .kept) : e.Has .ins ∧ ∃ p, e.Has (.rgn p) := by
  induction e <;> simp only [Tm.Has, reduceCtorEq, false_or, or_false] at h ⊢ <;> grind

/-- A step at the redex builds a kept handler only from one it holds: a cell holds none. -/
theorem RHead.kept {Λ μ e Λ' μ' e'} (hh : RHead Λ μ e Λ' μ' e')
    (hμ : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.NoIns) (h : e'.Has .kept) : e.Has .kept := by
  have cell : ∀ {l : Nat} {v x : Tm}, μ[l]? = some v → (∀ s, x.Has s → v.Has s) → ¬ x.Has .kept :=
    fun hl hx hk => hμ _ _ hl (hx _ (Tm.kept_ins hk).1)
  cases hh with
  | refGet hl => exact (cell hl (fun _ h => h) h).elim
  | forcePending hl => exact (cell hl (fun _ h => h) h).elim
  | forceDone hl => exact (cell hl (fun _ h => h) h).elim
  | settleVal _ | settleRaise _ | settleUnw | await | finRun => exact h
  | scopeRun => exact Tm.has_scopeRun h
  | tryRaise => exact h.elim .inl (.inr ·)
  | handleYes _ hm => exact (subst_match_has hm h).elim (fun h => .inr (.inl h)) .inl
  | handleNo => exact h.elim .inl (.inr ∘ .inr)
  | hguardNo => exact h.elim .inl (.inr ∘ .inr ∘ .inr)
  | handleNil => exact .inl h
  | close => exact .inr (.inr (.inr h))
  | hguardYes _ hm =>
    simp only [Tm.Has] at h ⊢
    rcases h with h | h | h
    · rcases subst_match_has hm h with h | h
      · exact .inr (.inl h)
      · exact .inl h
    · rcases subst_match_has hm h with h | h
      · exact .inr (.inr (.inl h))
      · exact .inl h
    · simp at h; grind
  | scope =>
    rcases h with h | h | h | h
    · cases h
    · cases h
    · cases h
    · rcases subst_inst_has h with h | ⟨w, hw, h⟩
      · exact h
      · simp at hw; subst hw; cases h
  | hkarmRaise | hkarmUnw => simp [Tm.Has]
  | @hctlRaise k p w _ _ ℓ κ E r al tl Ei _ hk _ =>
    subst hk
    have := plug_has_inv (r := .hole p) (r' := .unw p ℓ (.raise w)) (s := .kept) Ei
    simp only [Tm.Has] at h this ⊢; grind
  | @hctlUnw k p _ _ ℓ κ E r q ℓ' a al tl Ei _ hk =>
    subst hk
    have := plug_has_inv (r := .hole p) (r' := .unw p ℓ (.unw q ℓ' a)) (s := .kept) Ei
    simp only [Tm.Has] at h this ⊢; grind
  | @hresRaise a p w _ _ ℓ A _ ha _ =>
    subst ha
    have := plug_has_inv (r := .hole p) (r' := .raise w) (s := .kept) A
    simp only [Tm.Has] at h this ⊢; grind
  | @hresUnw a p _ _ ℓ q ℓ' b A _ ha =>
    subst ha
    have := plug_has_inv (r := .hole p) (r' := .unw q ℓ' b) (s := .kept) A
    simp only [Tm.Has] at h this ⊢; grind
  | hwithCatch | hdl => simp only [Tm.Has] at h ⊢; grind
  | _ => simp [Tm.Has] at h

/-- A step of a thread builds a kept handler only from one it holds: the roots and the cells
hold none. -/
theorem Step.kept {root l Λ μ e Λ' μ' e'} (hroot : RootBelow root)
    (hs : Step root l Λ μ e (.cfg Λ' μ' e')) (hμ : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.NoIns)
    (h : e'.Has .kept) : e.Has .kept := by
  cases hs with
  | pure hE hh => exact (plug_has_inv _ h).elim id fun h => plug_has _ (hh.has h)
  | rt hE hh => exact (plug_has_inv _ h).elim id fun h => plug_has _ (hh.kept hμ h)
  | glob hE hg => exact (plug_has_inv _ h).elim id fun h => (hroot.glob _ _ hg _ h).elim
  | addFin | spawn => exact (plug_has_inv _ h).elim id fun h => by simp [Tm.Has] at h
  | performRoot hE _ hr hf _ =>
    refine (plug_has_inv _ h).elim id fun h => ?_
    rcases h with h | h
    · obtain ⟨p, hp⟩ := (Tm.kept_ins (field_has hf h)).2
      exact (hroot.svc _ _ hr p hp).elim
    · exact plug_has _ (.inr h)
  | raise | unw => exact (plug_has_inv _ h).elim id fun h => plug_has _ (Frame.plug_has _ h)
  | performKept _ _ _ ha => exact (plug_has_inv _ h).elim id fun h => plug_has _ (kept_has ha h)
  | @performAbort E o b v Λ μ p ℓ r Ei Ef al _ _ _ ha =>
    refine (plug_has_inv _ h).elim id fun hs => plug_has _ ?_
    obtain ⟨h₁, h₂, h₃, h₄, -, h₆⟩ := perform_has (ℓ := ℓ) (c := .abort) (r := r)
      (x := .unw p ℓ (b.subst (inst [v]))) (δ := [v]) (s := .kept) ha
      (fun w hw hh => by simp at hw; rw [hw] at hh; exact .inr hh) (fun h => h)
    simp only [Tm.Has] at hs
    rcases hs with h | h | h | h | h
    · exact h₁ h
    · exact h₂ h
    · exact h₃ h
    · exact h₄ h
    · exact h₆ h
  | @performTail E o b v Λ μ p ℓ r Ei Ef al _ _ _ ha =>
    refine (plug_has_inv _ h).elim id fun hs => plug_has _ ?_
    obtain ⟨h₁, h₂, h₃, h₄, h₅, h₆⟩ :=
      perform_has (p := p) (ℓ := ℓ) (c := .tail) (r := r) (Ei := Ei) (x := .hole p) (δ := [v])
        (s := .kept) ha (fun w hw hh => by simp at hw; rw [hw] at hh; exact .inr hh)
        (fun h => h.elim .inl (.inr ∘ .inl))
    simp only [Tm.Has] at hs
    rcases hs with h | h | h | h | h | h
    · exact h₁ h
    · exact h₂ h
    · exact h₃ h
    · exact h₄ h
    · exact h₅ h
    · exact h₆ h
  | @performCtl E o b v Λ μ p ℓ r Ei Ef al _ _ _ ha =>
    refine (plug_has_inv _ h).elim id fun hs => plug_has _ ?_
    obtain ⟨h₁, h₂, h₃, h₄, h₅, h₆⟩ :=
      perform_has (p := p) (ℓ := ℓ) (c := .control) (r := r) (Ei := Ei) (x := .hole p)
        (δ := [v, .kont p μ.length]) (s := .kept) ha
        (fun w hw hh => by
          simp at hw
          rcases hw with hw | hw <;> rw [hw] at hh
          · exact .inr hh
          · simp [Tm.Has] at hh)
        (fun h => h.elim .inl (.inr ∘ .inl))
    simp only [Tm.Has] at hs
    rcases hs with h | h | h | h | h | h
    · exact h₁ h
    · exact h₂ h
    · exact h₃ h
    · exact h₄ h
    · exact h₅ h
    · exact h₆ h
  | @resume E A k p u Λ μ ℓ r Ef al Ei ℓh _ _ _ hk _ _ =>
    subst hk
    refine (plug_has_inv _ h).elim id fun hs => plug_has _ ?_
    simp only [Tm.Has] at hs ⊢
    rcases hs with h | h | h | h | h | h | h | h
    · exact .inl h
    · exact .inr (.inl h)
    · rcases plug_has_inv (r := .app (.kont p ℓ) u) A h with h | h
      · exact .inr (.inr (.inr (.inr (.inl h))))
      · simp only [Tm.Has] at h
        rcases h with h | h
        · exact .inl h
        · exact .inr (.inl h)
    · exact .inl h
    · exact .inr (.inl h)
    · exact .inr (.inr (.inl h))
    · exact .inr (.inr (.inr (.inl h)))
    · rcases plug_has_inv (r := .hole p) Ei h with h | h
      · exact .inr (.inr (.inr (.inr (.inr h))))
      · exact .inr (.inr (.inr (.inr (.inl (plug_has A (by simp [Tm.Has, h]))))))

end Flick
