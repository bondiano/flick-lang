import Formal.MainKept

/-!
# T1: type safety (core.md §4) with regions

A configuration is typed when its store is, its main thread has the program's
type, and every child and finalizer of every region has some type. A step of the
main thread or of a child keeps this, and the main thread is a value, `raise v`,
steps, or is blocked: it waits for a child of a region it closes, or uses a
region that is not live. T4′ shows the second never happens (`T1_regions`).
-/

namespace Flick

variable {S : Sig}

/-- A configuration step keeps a typed configuration at the same type and stage,
with a store typing that extends the first one. The type and the stage of the
program name no run-time region. -/
theorem cstep_typed {root Ψ c c' t φ l} (hroot : RootTy S root)
    (ht : t.YF 0)
    (hφ : φ.substT (kill 0) = φ) (hc : CfgTy S Ψ c t φ) (hs : CStep root l c c') :
    ∃ X, CfgTy S (Ψ ++ X) c' t φ ∧ ∀ u ∈ X, u.YF c.rgs.length := by
  obtain ⟨hμ, h, hΛ, hb, hΨ, ha, hcs, hcc, hcg⟩ := hc
  cases hs with
  | main hs =>
    obtain ⟨X, hμ', h', hX⟩ := preservation hroot h hμ hb.main hb.mem hΨ hs
    obtain ⟨hb', hl⟩ := hs.below hroot.below hb
    obtain ⟨ha', hka⟩ := hs.asc hroot ha hb.main
    obtain ⟨hs', cell', -, hcc'⟩ := hs.caught hroot hcg ha hb (Nat.zero_le _)
    rw [ht.mono (Nat.zero_le _), Stage.YF_mono hφ (Nat.zero_le _)] at h'
    refine ⟨X, ⟨hμ', h', (step_rgs hΛ h hb.main hΨ ha.main hs).weaken X, hb', fun u hu => ?_, hka _ ha',
      hs', cell', hcc'⟩, hX⟩
    rcases List.mem_append.mp hu with hu | hu
    · exact (hΨ u hu).mono hl
    · exact (hX u hu).mono hl
  | kid hg _ hj hs =>
    have hkm := List.mem_of_getElem? hj
    obtain ⟨t', φ', hk⟩ := (hΛ _ _ hg).1 _ hkm
    have hkb := (hb.rgs _ _ hg).1 _ hkm
    have hkA := ha.rgs _ _ hg _ (List.mem_append_left _ hkm)
    obtain ⟨X, hμ', hk', hX⟩ := preservation hroot hk hμ hkb hb.mem hΨ hs
    obtain ⟨hb', hl⟩ := hs.below hroot.below ⟨hkb, hb.mem, hb.rgs⟩
    obtain ⟨ha', hka⟩ := hs.asc hroot ⟨hkA, ha.rgs, ha.mem⟩ hkb
    obtain ⟨m, hm, hkm'⟩ := hcg.rgs _ _ hg _ (List.mem_append_left _ hkm)
    obtain ⟨hs', cell', hg', hkc⟩ := hs.caught hroot (n := m)
      ⟨hkm', hcg.rgs⟩ ⟨hkA, ha.rgs, ha.mem⟩ ⟨hkb, hb.mem, hb.rgs⟩ hm
    have hll := (hs.below hroot.below ⟨hkb, hb.mem, hb.rgs⟩).2
    refine ⟨X, ⟨hμ', h.weakenStore X, ((step_rgs hΛ hk hkb hΨ hkA hs).weaken X).setKid ⟨_, _, hk'⟩,
      CfgBelow.kid ⟨hb.main.mono hl, hb'.mem, hb'.rgs⟩ hb'.main, fun u hu => ?_,
      (hka _ ha.main).setKid ha', hs', cell', hkc.setKid (Nat.le_trans hm hll) hkc.main (hcg.main.ghostB hb.main hg')⟩, hX⟩
    rw [setKid_length]
    rcases List.mem_append.mp hu with hu | hu
    · exact (hΨ u hu).mono hl
    · exact (hX u hu).mono hl

theorem steps_typed {root c c'} (hroot : RootTy S root)
    (hs : CSteps root c c') :
    ∀ {Ψ t φ}, t.YF 0 → φ.substT (kill 0) = φ → CfgTy S Ψ c t φ →
      ∃ X, CfgTy S (Ψ ++ X) c' t φ := by
  induction hs with
  | refl => intro Ψ _ _ _ _ h; exact ⟨[], by simpa using h⟩
  | step h₁ _ ih =>
    intro _ _ _ ht hφ h
    obtain ⟨X₁, h', -⟩ := cstep_typed hroot ht hφ h h₁
    obtain ⟨X₂, h''⟩ := ih ht hφ h'
    exact ⟨X₁ ++ X₂, by simpa using h''⟩

/-- A raised value has a type of errors within those of the stage. -/
theorem raise_errors {Ψ Θ v t φ} (hv : Value v) (h : Typ S Ψ Θ [] (.raise v) (.ex t φ)) :
    ∃ ε, Row.Sub ε.asRow φ.err ∧ Typ S Ψ Θ [] v (.ex (.errors ε) .pure) := by
  obtain ⟨ε, hv', hε⟩ := raise_inv hv h
  obtain ⟨_, _, _, _, U, _, _, _, hU, ht, hsub⟩ := canonical_errors hv hv'
  exact ⟨.rnm _ U, hsub.trans (Stage.le_err hε), .errVal ht hU (Row.Sub.refl _)⟩

/-- The initial configuration of a program. -/
theorem CfgTy.init {e₀ t φ₀} (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀)) (hb : e₀.Below 0) :
    CfgTy S [] ⟨[], [], e₀⟩ t φ₀ :=
  ⟨⟨rfl, by simp⟩, h₀.cut (Entails.none _), fun _ _ h => by simp at h,
    ⟨hb, fun _ _ h => by simp at h, fun _ _ h => by simp at h⟩, by simp,
    ⟨Tm.Asc.of_noIns hb.noIns, fun _ _ h => by simp at h, fun _ _ h => by simp at h⟩,
    fun _ _ => False, fun _ => 0, ⟨Tm.Caught.noIns hb.noIns, fun _ _ h => by simp at h⟩⟩

/-- A root of an effect answers each of its operations: an effect with one that is not `tail`
has no root. -/
theorem root_answers {root Ef o c a b w} (hroot : RootTy S root) (hw : root.svc Ef = some w)
    (hop : S.op Ef o = some (c, a, b)) : RootAnswers root Ef o c := by
  obtain ⟨hwv, hwt⟩ := hroot.svc _ _ hw
  have hne : S.impl Ef ≠ .errors .cnil := fun he => by
    rw [he] at hwt
    obtain ⟨_, _, _, _, _, _, _, hN, _⟩ := canonical_errors hwv hwt
    exact hN.elim
  have hc : c = .tail := Classical.byContradiction fun hc => hne (S.op_impl _ _ _ _ _ hop hc)
  subst hc
  obtain ⟨⟨f, hf⟩, _⟩ := (hwt.shape hwv rfl).1.field rfl (S.op_field _ _ _ _ hop hne)
  exact ⟨rfl, w, f, hw, hf⟩

/-- Steps keep the main thread without a kept handler: only a child has one. -/
theorem steps_kept {root c c'} (hroot : RootTy S root) (hs : CSteps root c c') :
    ∀ {Ψ t φ}, t.YF 0 → φ.substT (kill 0) = φ → CfgTy S Ψ c t φ → ¬ c.main.Has .kept →
      ¬ c'.main.Has .kept := by
  induction hs with
  | refl => intro _ _ _ _ _ _ h; exact h
  | step h₁ _ ih =>
    intro _ _ _ ht hφ hc hk
    obtain ⟨X, hc', -⟩ := cstep_typed hroot ht hφ hc h₁
    refine ih ht hφ hc' ?_
    cases h₁ with
    | main hs => exact fun h => hk (hs.kept hroot.below hc.2.2.2.2.2.1.mem h)
    | kid => exact hk

/-- **T1.** A program `· ⊢ e₀ : τ ! φ₀` (a term without run-time regions, at a type and
a stage without them) whose services `Σ₀` provides (G3) reaches
only configurations whose main thread is a value, `raise v` with `v : Errors ε₀`,
makes a step, or is blocked (waits for a child of a region it closes, or uses a
region that is not live); every child of a live region is finished, makes a step,
performs an operation no handler and no root answers, or one that is not `tail` at a handler
it keeps, or is blocked. Neither operation is an outcome of the main thread: `Σ₀` has a
function for each operation of an effect of the stage, and only a child keeps a handler
(`steps_kept`). Nor does it unwind past its top: every signal is under the frame of its
handler (`CfgCaught`). -/
theorem T1 {root e₀ t φ₀ c} (hroot : RootTy S root)
    (hcov : ∀ N c, φ₀.svc (.nm N c) → (root.svc N).isSome) (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀))
    (hp : e₀.Below 0) (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hs : CSteps root ⟨[], [], e₀⟩ c) :
    ((Value c.main ∧ ∃ Ψ, StoreTy S Ψ c.mem ∧ Typ S Ψ Facts.age [] c.main (.ex t .pure)) ∨
      (∃ v, c.main = .raise v ∧ ∃ Ψ ε, Row.Sub ε.asRow φ₀.err ∧
        Typ S Ψ Facts.age [] v (.ex (.errors ε) .pure)) ∨
      (∃ l o, Step root l c.rgs c.mem c.main o) ∨ Blocked c.rgs c.main) ∧
    ∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → g.live = true → ∀ k ∈ g.kids,
      Final k ∨ (∃ l o, Step root l c.rgs c.mem k o) ∨ (∃ D, PerformStuck S root k D) ∨
        Blocked c.rgs k := by
  obtain ⟨Ψ, hμ, h, hΛ, -, -, -, _, _, hcg⟩ := steps_typed hroot hs ht hφ (CfgTy.init h₀ hp)
  have hkept := steps_kept hroot hs ht hφ (CfgTy.init h₀ hp) fun h =>
    let ⟨p, hp'⟩ := (Tm.kept_ins h).2; Nat.not_lt_zero _ (hp p hp')
  simp only [List.nil_append] at hμ h hΛ
  refine ⟨?_, fun p g hg _ k hk => ?_⟩
  · rcases progress (Λ := c.rgs) (root := root) hroot h hμ.1 rfl rfl with hv | hn
    · exact .inl ⟨hv, Ψ, hμ, h.valueAny hv _⟩
    cases hn with
    | raise h' =>
      obtain ⟨v, hv, he⟩ := h'
      obtain ⟨ε, hε, hv'⟩ := raise_errors hv (he ▸ h)
      exact .inr (.inl ⟨v, he, Ψ, ε, hε, hv'⟩)
    | step hstep => exact .inr (.inr (.inl hstep))
    | perform h' =>
      rcases h' with ⟨_, Ef, o, c', _, a, b, _, _, _, hop, _, hna, _, hD⟩ |
        ⟨E, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, he⟩
      · obtain ⟨w, hw⟩ := Option.isSome_iff_exists.mp (hcov Ef _ hD)
        exact (hna (root_answers hroot hw hop)).elim
      · exact (hkept (he ▸ plug_has E (by simp [Tm.Has]))).elim
    | blocked hb => exact .inr (.inr (.inr hb))
    | unw h' =>
      obtain ⟨_, _, _, he⟩ := h'
      exact (Nat.not_lt_zero _ (he ▸ hcg.main).1).elim
  · obtain ⟨t', φ', hk'⟩ := (hΛ p g hg).1 k hk
    rcases progress (Λ := c.rgs) (root := root) hroot hk' hμ.1 rfl rfl with hv | hn
    · exact .inl (.inl hv)
    cases hn with
    | raise h' => exact .inl (.inr (.inl h'))
    | step hstep => exact .inr (.inl hstep)
    | perform hp => exact .inr (.inr (.inl ⟨_, hp⟩))
    | blocked hb => exact .inr (.inr (.inr hb))
    | unw h' => exact .inl (.inr (.inr h'))

end Flick
