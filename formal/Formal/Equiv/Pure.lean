import Formal.Equiv.Top
import Formal.Decomp
import Formal.Elab.CoreSim

/-!
# Contextual equivalence: pure evaluation and determinism

A pure step (`PStep`, `Formal/Decomp.lean`) is a pure head step (`PHead`) or the unfolding of a
global (`Step.glob`); it reads and changes neither the store nor the regions, and is deterministic
(`PStep.det`). Its reflexive-transitive closure `PSteps` therefore reaches at most one value
(`PSteps.det`) and replays in any store and regions (`PSteps.steps`).

`PureEval root e` states that `e` is operationally pure: every step from any `PSteps`-reachable
state is a `PStep` that keeps the store and regions, and it reaches no `raise v`, `unw`, `settle`,
`perform`, or `kont` application in an ordered context, nor a lazy cell where a pattern reads
(`Tm.LzTop`).
-/

namespace Flick.Equiv

/-- Multi-step pure evaluation: pure head steps and global unfoldings. -/
inductive PSteps (root : Roots) : Tm → Tm → Prop
  | refl {e} : PSteps root e e
  | step {e e₁ e'} : PStep root e e₁ → PSteps root e₁ e' → PSteps root e e'

theorem PSteps.trans {root : Roots} {a b c : Tm} (h₁ : PSteps root a b) (h₂ : PSteps root b c) :
    PSteps root a c := by
  induction h₁ with
  | refl => exact h₂
  | step s _ ih => exact .step s (ih h₂)

theorem PSteps.single {root : Roots} {a b : Tm} (h : PStep root a b) : PSteps root a b :=
  .step h .refl

/-- **Determinism of pure evaluation**: a pure run reaches at most one value. -/
theorem PSteps.det {root : Roots} {e v₁ v₂ : Tm} (h₁ : PSteps root e v₁) (h₂ : PSteps root e v₂)
    (hv₁ : Value v₁) (hv₂ : Value v₂) : v₁ = v₂ := by
  induction h₁ with
  | refl =>
    cases h₂ with
    | refl => rfl
    | step s _ => exact (s.not_value hv₁).elim
  | step s₁ _ ih =>
    cases h₂ with
    | refl => exact (s₁.not_value hv₂).elim
    | step s₂ h₂' => exact ih (PStep.det s₁ s₂ ▸ h₂') hv₁

theorem PSteps.frame {root : Roots} {a b : Tm} (h : PSteps root a b) {F : Frame} (hF : F.ok) :
    PSteps root (F.plug a) (F.plug b) := by
  induction h with
  | refl => exact .refl
  | step s _ ih => exact .step (s.frame hF) ih

theorem PSteps.plug {root : Roots} {a b : Tm} (h : PSteps root a b) :
    ∀ {E : List Frame}, OkE E → PSteps root (Flick.plug E a) (Flick.plug E b)
  | [], _ => h
  | F :: E, hE => by
    simp only [Flick.plug]
    exact PSteps.plug (h.frame hE.cons.1) hE.cons.2

/-- A pure run replays in any store and regions without changing them. -/
theorem PSteps.steps {root : Roots} {a b : Tm} (h : PSteps root a b) (Λ : List Rgn) (μ : List Tm) :
    Steps root (· = .pure) Λ μ a Λ μ b := by
  induction h with
  | refl => exact .refl
  | step s _ ih => exact .step (s.step Λ μ) rfl ih

theorem step_not_value {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' : List Tm} {t t' : Tm}
    (hs : Step root l Λ μ t (.cfg Λ' μ' t')) : ¬ Value t := fun hv => by
  cases hs with
  | pure hE hh => exact not_value_plugs hh.not_value _ hv
  | rt hE hh => exact not_value_plugs hh.not_value _ hv
  | addFin hE => exact not_value_plugs (Flick.Elab.fin_redex (.inl rfl) ‹_›).1 _ hv
  | spawn hE => exact not_value_plugs (Flick.Elab.fin_redex (.inr rfl) ‹_›).1 _ hv
  | performRoot hE => exact not_value_plugs (Flick.Elab.perform_redex ‹_›).1 _ hv
  | raise hE => exact not_value_plugs (not_value_plug (Flick.Elab.raise_redex ‹_›).1 _) _ hv
  | unw hE => exact not_value_plugs (not_value_plug (Flick.Elab.unw_redex _ _ _).1 _) _ hv
  | performAbort hE | performTail hE | performKept hE | performCtl hE | resume hE =>
    exact not_value_plugs (fun h => by cases h) _ hv
  | glob hE => exact not_value_plugs (glob_redex _).1 _ hv

/-- Every `match` or `handle` redex in `t` binds only values (which typing guarantees). -/
def _root_.Flick.Tm.MatchVals (t : Tm) : Prop :=
  ∀ E v p b rest δ, OkE E → Value v → Flick.matchP p v = some δ →
    (t = plug E (.match_ v (.acons p b rest)) ∨
     (∃ g, t = plug E (.match_ v (.aguard p g b rest))) ∨
     t = plug E (.handle v (.acons p b rest)) ∨
     (∃ g, t = plug E (.handle v (.aguard p g b rest)))) →
    ∀ w ∈ δ, Value w

theorem _root_.Flick.Tm.MatchVals.plug_inner {E : List Frame} {r : Tm}
    (h : (plug E r).MatchVals) (hE : OkE E) : r.MatchVals := by
  intro E₀ v p b rest δ hE₀ hv hm hr
  refine h (E₀ ++ E) v p b rest δ (hE₀.append' hE) hv hm ?_
  rcases hr with rfl | ⟨g, rfl⟩ | rfl | ⟨g, rfl⟩
  · exact .inl (plug_append E₀ E _).symm
  · exact .inr (.inl ⟨g, (plug_append E₀ E _).symm⟩)
  · exact .inr (.inr (.inl (plug_append E₀ E _).symm))
  · exact .inr (.inr (.inr ⟨g, (plug_append E₀ E _).symm⟩))

/-- Operational purity of a closed term: every step from any `PSteps`-reachable state is a `PStep`
that keeps the store and regions, and it reaches no `raise v`, `unw`, `settle`, `perform`, or
`kont` application in an ordered context, nor any lazy cell where a pattern reads, and every
pattern match binds only values. -/
def PureEval (root : Roots) (e : Tm) : Prop :=
  ∀ t, PSteps root e t →
    (∀ Λ μ l Λ' μ' t', Step root l Λ μ t (.cfg Λ' μ' t') →
      l = .pure ∧ Λ' = Λ ∧ μ' = μ ∧ PStep root t t') ∧
    (∀ E v, OkE E → Value v → t ≠ plug E (.raise v)) ∧
    (∀ E p ℓ a, OkE E → t ≠ plug E (.unw p ℓ a)) ∧
    (∀ E l a, OkE E → t ≠ plug E (.settle l a)) ∧
    (∀ E Ef o c v, OkE E → Value v → t ≠ plug E (.app (.const (.perform Ef o c)) v)) ∧
    (∀ E p ℓ u, OkE E → Value u → t ≠ plug E (.app (.kont p ℓ) u)) ∧
    ¬ t.LzTop .any ∧ t.MatchVals

theorem PureEval.step {root : Roots} {e e' : Tm} (h : PureEval root e) (hs : PStep root e e') :
    PureEval root e' :=
  fun t ht => h t (.step hs ht)

theorem PureEval.steps {root : Roots} {e e' : Tm} (h : PureEval root e) (hs : PSteps root e e') :
    PureEval root e' :=
  fun t ht => h t (hs.trans ht)

theorem PureEval.step_pure {root : Roots} {e t : Tm} (h : PureEval root e) (ht : PSteps root e t)
    {Λ μ l Λ' μ' t'} (hs : Step root l Λ μ t (.cfg Λ' μ' t')) :
    l = .pure ∧ Λ' = Λ ∧ μ' = μ ∧ PStep root t t' :=
  (h t ht).1 Λ μ l Λ' μ' t' hs

theorem PureEval.no_raise {root : Roots} {e t : Tm} (h : PureEval root e) (ht : PSteps root e t)
    (E : List Frame) (v : Tm) (hE : OkE E) (hv : Value v) : t ≠ plug E (.raise v) :=
  (h t ht).2.1 E v hE hv

theorem PureEval.no_unw {root : Roots} {e t : Tm} (h : PureEval root e) (ht : PSteps root e t)
    (E : List Frame) (p ℓ : Nat) (a : Tm) (hE : OkE E) : t ≠ plug E (.unw p ℓ a) :=
  (h t ht).2.2.1 E p ℓ a hE

theorem PureEval.no_settle {root : Roots} {e t : Tm} (h : PureEval root e) (ht : PSteps root e t)
    (E : List Frame) (l : Nat) (a : Tm) (hE : OkE E) : t ≠ plug E (.settle l a) :=
  (h t ht).2.2.2.1 E l a hE

theorem PureEval.no_perform {root : Roots} {e t : Tm} (h : PureEval root e) (ht : PSteps root e t)
    (E : List Frame) (Ef o : Name) {c} (v : Tm) (hE : OkE E) (hv : Value v) :
    t ≠ plug E (.app (.const (.perform Ef o c)) v) :=
  (h t ht).2.2.2.2.1 E Ef o c v hE hv

theorem PureEval.no_kont {root : Roots} {e t : Tm} (h : PureEval root e) (ht : PSteps root e t)
    (E : List Frame) (p ℓ : Nat) (u : Tm) (hE : OkE E) (hu : Value u) :
    t ≠ plug E (.app (.kont p ℓ) u) :=
  (h t ht).2.2.2.2.2.1 E p ℓ u hE hu

theorem PureEval.noLzTop {root : Roots} {e t : Tm} (h : PureEval root e) (ht : PSteps root e t) :
    ¬ t.LzTop .any :=
  (h t ht).2.2.2.2.2.2.1

theorem PureEval.matchVals {root : Roots} {e t : Tm} (h : PureEval root e) (ht : PSteps root e t) :
    t.MatchVals :=
  (h t ht).2.2.2.2.2.2.2

theorem PureEval.not_lazy {root : Roots} {e : Tm} (h : PureEval root (.lazy e)) : False := by
  have := (h.step_pure .refl (Step.rt (root := root) (Λ := []) (μ := []) OkE.nil (.lazy (e := e)))).1
  cases this

theorem PureEval.not_let_lazy {root : Roots} {e b : Tm} (h : PureEval root (.let_ (.lazy e) b)) :
    False := by
  have := (h.step_pure .refl (Step.rt (root := root) (Λ := []) (μ := [])
    (E := [.let_ b]) (OkE.nil.cons' trivial) (.lazy (e := e)))).1
  cases this

theorem PureEval.not_force_lazy {root : Roots} {e : Tm} (h : PureEval root (.force (.lazy e))) :
    False := by
  have := (h.step_pure .refl (Step.rt (root := root) (Λ := []) (μ := [])
    (E := [.force]) (OkE.nil.cons' trivial) (.lazy (e := e)))).1
  cases this

theorem PureEval.not_settle {root : Roots} {l : Nat} {a : Tm} (h : PureEval root (.settle l a)) :
    False :=
  h.no_settle .refl [] l a OkE.nil rfl

/-- A term inside an ordered frame of a pure term is pure. -/
theorem PureEval.of_frame {root : Roots} {F : Frame} {a : Tm} (hF : F.ok)
    (h : PureEval root (F.plug a)) : PureEval root a := by
  intro t ht
  have h2 := h (F.plug t) (ht.frame hF)
  refine ⟨fun Λ μ l Λ' μ' t' hs => ?_, fun E v hE hv he => ?_, fun E p ℓ c hE he => ?_,
    fun E l c hE he => ?_, fun E Ef o c v hE hv he => ?_, fun E p ℓ u hE hu he => ?_,
    fun hz => h2.2.2.2.2.2.2.1 (hz.frame F), fun E v p b rest δ hE hv hm ht =>
      h2.2.2.2.2.2.2.2 (E ++ [F]) v p b rest δ (hE.append hF) hv hm (by
        rcases ht with rfl | ⟨g, rfl⟩ | rfl | ⟨g, rfl⟩
        · exact .inl (by rw [plug_snoc])
        · exact .inr (.inl ⟨g, by rw [plug_snoc]⟩)
        · exact .inr (.inr (.inl (by rw [plug_snoc])))
        · exact .inr (.inr (.inr ⟨g, by rw [plug_snoc]⟩)))⟩
  · cases hs with
    | pure hE hh => exact ⟨rfl, rfl, rfl, .head hE hh⟩
    | glob hE hg => exact ⟨rfl, rfl, rfl, .glob hE hg⟩
    | rt hE hh =>
      have := (h2.1 Λ μ .rt Λ' μ' _ (by rw [← plug_snoc]; exact .rt (hE.append hF) hh)).1
      cases this
    | addFin hE hg hl hv =>
      have := (h2.1 Λ μ .rt _ μ _ (by rw [← plug_snoc]; exact .addFin (hE.append hF) hg hl hv)).1
      cases this
    | spawn hE hg hl hv =>
      have := (h2.1 Λ μ .rt _ μ _ (by rw [← plug_snoc]; exact .spawn (hE.append hF) hg hl hv)).1
      cases this
    | performRoot hE _ _ _ hv =>
      exact (h2.2.2.2.2.1 (_ ++ [F]) _ _ _ _ (hE.append hF) hv (by rw [plug_snoc])).elim
    | raise hE hF' hnt hni hnc hv =>
      have := (h2.1 Λ μ .rt Λ μ _ (by rw [← plug_snoc]; exact .raise (hE.append hF) hF' hnt hni hnc hv)).1
      cases this
    | unw hE hF' hni hnc hnh =>
      have := (h2.1 Λ μ .rt Λ μ _ (by rw [← plug_snoc]; exact .unw (hE.append hF) hF' hni hnc hnh)).1
      cases this
    | performAbort hE hEi hh ha hv =>
      have := (h2.1 Λ μ .rt Λ μ _ (by rw [← plug_snoc]; exact .performAbort (hE.append hF) hEi hh ha hv)).1
      cases this
    | performTail hE hEi hh ha hv =>
      have := (h2.1 Λ μ .rt Λ μ _ (by rw [← plug_snoc]; exact .performTail (hE.append hF) hEi hh ha hv)).1
      cases this
    | performKept hE hEi hh ha hv hq =>
      have := (h2.1 Λ μ .rt Λ μ _ (by rw [← plug_snoc]; exact .performKept (hE.append hF) hEi hh ha hv hq)).1
      cases this
    | performCtl hE hEi hh ha hv =>
      have := (h2.1 Λ μ .rt Λ _ _ (by rw [← plug_snoc]; exact .performCtl (hE.append hF) hEi hh ha hv)).1
      cases this
    | resume hE hA hEi hk hn hu =>
      have := (h2.1 Λ μ .rt Λ μ _ (by rw [← plug_snoc]; exact .resume (hE.append hF) hA hEi hk hn hu)).1
      cases this
  · exact h2.2.1 (E ++ [F]) v (hE.append hF) hv (by rw [plug_snoc, he])
  · exact h2.2.2.1 (E ++ [F]) p ℓ c (hE.append hF) (by rw [plug_snoc, he])
  · exact h2.2.2.2.1 (E ++ [F]) l c (hE.append hF) (by rw [plug_snoc, he])
  · exact h2.2.2.2.2.1 (E ++ [F]) Ef o c v (hE.append hF) hv (by rw [plug_snoc, he])
  · exact h2.2.2.2.2.2.1 (E ++ [F]) p ℓ u (hE.append hF) hu (by rw [plug_snoc, he])

/-- If an ordered context around a redex equals an ordered context around a non-value, then the
outer context is a suffix of the redex's context. -/
theorem decomp_plug_nonval {r a : Tm} {E₁ : List Frame} (hr : Redex r) (ha : ¬ Value a) :
    ∀ {E : List Frame}, OkE E → OkE E₁ → plug E r = plug E₁ a →
      ∃ E₀, E = E₀ ++ E₁ ∧ OkE E₀ ∧ a = plug E₀ r := by
  induction E₁ generalizing a with
  | nil =>
    intro E hE _ hq
    exact ⟨E, by simp, hE, hq.symm⟩
  | cons F E₁ ih =>
    intro E hE hE₁ hq
    simp only [Flick.plug] at hq
    obtain ⟨E₀, rfl, hE₀, hFa⟩ := ih (not_value_plug ha F) hE hE₁.cons.2 hq
    obtain ⟨E₀', rfl, hE₀', ha'⟩ := decomp_frame hr hE₀ hE₁.cons.1 ha hFa.symm
    exact ⟨E₀', by simp, hE₀', ha'⟩

/-- A step of a pure non-value inside `settle l₀` must be a `PStep` of that pure term. -/
theorem PureEval.step_settle {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' : List Tm}
    {E₁ : List Frame} {l₀ : Nat} {e t t' : Tm}
    (hE₁ : OkE E₁) (hpe : PureEval root e) (hpt : PSteps root e t) (hnv : ¬ Value t)
    (hs : Step root l Λ μ (plug E₁ (.settle l₀ t)) (.cfg Λ' μ' t')) :
    l = .pure ∧ Λ' = Λ ∧ μ' = μ ∧ ∃ t₁, PStep root t t₁ ∧ t' = plug E₁ (.settle l₀ t₁) := by
  have hnvS : ¬ Value (.settle l₀ t) := fun hv => by cases hv
  have split_redex : ∀ {E : List Frame} {r : Tm}, OkE E → Redex r →
      (∀ n a, r ≠ .settle n a) → plug E r = plug E₁ (.settle l₀ t) →
      ∃ E₀, E = E₀ ++ .settle l₀ :: E₁ ∧ OkE E₀ ∧ t = plug E₀ r := by
    intro E r hE hr hns hq
    obtain ⟨E', rfl, hE', hst⟩ := decomp_plug_nonval hr hnvS hE hE₁ hq
    rcases snoc_or E' with rfl | ⟨E₀, F, rfl⟩
    · simp only [plug] at hst; exact (hns l₀ t hst.symm).elim
    · rw [plug_snoc] at hst
      obtain ⟨rfl, ht⟩ := frame_inj (F := .settle l₀) trivial (hE' F (by simp)) hnv
        (not_value_plugs hr.1 _) hst
      exact ⟨E₀, by simp, fun G hG => hE' G (by simp [hG]), ht⟩
  have split_raise : ∀ {E : List Frame} {F : Frame} {v : Tm}, OkE E → F.ok → Value v →
      plug E (F.plug (.raise v)) = plug E₁ (.settle l₀ t) → False := by
    intro E F v hE hF hv hq
    obtain ⟨E₀, _, hE₀, ht⟩ :=
      split_redex (hE.cons' hF) (Flick.Elab.raise_redex hv) (fun _ _ h => by cases h) hq
    exact hpe.no_raise hpt E₀ v hE₀ hv ht
  have split_unw : ∀ {E : List Frame} {F : Frame} {p ℓ : Nat} {a : Tm}, OkE E → F.ok →
      plug E (F.plug (.unw p ℓ a)) = plug E₁ (.settle l₀ t) → False := by
    intro E F p ℓ a hE hF hq
    obtain ⟨E₀, _, hE₀, ht⟩ :=
      split_redex (hE.cons' hF) (Flick.Elab.unw_redex p ℓ a) (fun _ _ h => by cases h) hq
    exact hpe.no_unw hpt E₀ p ℓ a hE₀ ht
  generalize hq : plug E₁ (.settle l₀ t) = q at hs
  cases hs with
  | pure hE hh =>
    obtain ⟨E₀, rfl, hE₀, rfl⟩ :=
      split_redex hE hh.redex (fun _ _ h => by subst h; cases hh) hq.symm
    exact ⟨rfl, rfl, rfl, _, .head hE₀ hh, by rw [plug_append]; rfl⟩
  | glob hE hg =>
    obtain ⟨E₀, rfl, hE₀, rfl⟩ :=
      split_redex hE (glob_redex _) (fun _ _ h => by cases h) hq.symm
    exact ⟨rfl, rfl, rfl, _, .glob hE₀ hg, by rw [plug_append]; rfl⟩
  | rt hE hh =>
    rename_i e₀ e₀'
    rcases hh.redex' with hr | ⟨F, v, hF, hv, rfl⟩ | ⟨F, p, ℓ, a, hF, rfl⟩
    · by_cases hst : ∃ n a, e₀ = Tm.settle n a
      · obtain ⟨n, a, rfl⟩ := hst
        obtain ⟨E', rfl, hE', hEq⟩ := decomp_plug_nonval hr hnvS hE hE₁ hq.symm
        rcases snoc_or E' with rfl | ⟨E₀, F, rfl⟩
        · simp only [plug] at hEq
          cases hEq
          cases hh with
          | settleVal hv => exact (hnv hv).elim
          | settleRaise hv => exact (hpe.no_raise hpt [] _ OkE.nil hv rfl).elim
          | settleUnw => exact (hpe.no_unw hpt [] _ _ _ OkE.nil rfl).elim
        · rw [plug_snoc] at hEq
          obtain ⟨rfl, ht⟩ := frame_inj (F := .settle l₀) trivial (hE' F (by simp)) hnv
            (not_value_plugs hr.1 _) hEq
          exact (hpe.no_settle hpt E₀ n a (fun G hG => hE' G (by simp [hG])) ht).elim
      · obtain ⟨E₀, -, hE₀, rfl⟩ :=
          split_redex hE hr (fun n a h => hst ⟨n, a, h⟩) hq.symm
        have := (hpe.step_pure hpt (.rt hE₀ hh)).1
        cases this
    · exact (split_raise hE hF hv hq.symm).elim
    · exact (split_unw hE hF hq.symm).elim
  | addFin hE hg hl hv =>
    obtain ⟨E₀, -, hE₀, rfl⟩ :=
      split_redex hE (Flick.Elab.fin_redex (.inl rfl) hv) (fun _ _ h => by cases h) hq.symm
    have := (hpe.step_pure hpt (.addFin (μ := μ) hE₀ hg hl hv)).1; cases this
  | spawn hE hg hl hv =>
    obtain ⟨E₀, -, hE₀, rfl⟩ :=
      split_redex hE (Flick.Elab.fin_redex (.inr rfl) hv) (fun _ _ h => by cases h) hq.symm
    have := (hpe.step_pure hpt (.spawn (μ := μ) hE₀ hg hl hv)).1; cases this
  | performRoot hE hh hr hf hv =>
    obtain ⟨E₀, -, hE₀, ht⟩ :=
      split_redex hE (Flick.Elab.perform_redex hv) (fun _ _ h => by cases h) hq.symm
    exact (hpe.no_perform hpt E₀ _ _ _ hE₀ hv ht).elim
  | raise hE hF _ _ _ hv => exact (split_raise hE hF hv hq.symm).elim
  | unw hE hF => exact (split_unw hE hF hq.symm).elim
  | performAbort hE hEi _ _ hv | performTail hE hEi _ _ hv | performCtl hE hEi _ _ hv =>
    obtain ⟨E₀, -, hE₀, ht⟩ := split_redex (hEi.append' (hE.cons' (F := .hwith _ _ _ _ _) trivial))
      (Flick.Elab.perform_redex hv) (fun _ _ h => by cases h)
      ((plug_append _ (.hwith _ _ _ _ _ :: _) _).trans hq.symm)
    exact (hpe.no_perform hpt E₀ _ _ _ hE₀ hv ht).elim
  | performKept hE hEi _ _ hv _ =>
    obtain ⟨E₀, -, hE₀, ht⟩ := split_redex (hEi.append' (hE.cons' (F := .hkeep _ _ _ _) trivial))
      (Flick.Elab.perform_redex hv) (fun _ _ h => by cases h)
      ((plug_append _ (.hkeep _ _ _ _ :: _) _).trans hq.symm)
    exact (hpe.no_perform hpt E₀ _ _ _ hE₀ hv ht).elim
  | resume hE hA _ _ _ hu =>
    obtain ⟨E₀, -, hE₀, ht⟩ := split_redex (hA.append' (hE.cons' (F := .hctlA _ _ _ _ _ _ _ _) trivial))
      (Flick.Elab.kont_redex hu) (fun _ _ h => by cases h)
      ((plug_append _ (.hctlA _ _ _ _ _ _ _ _ :: _) _).trans hq.symm)
    exact (hpe.no_kont hpt E₀ _ _ _ hE₀ hu ht).elim


end Flick.Equiv
