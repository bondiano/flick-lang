import Formal.Preservation
import Formal.BelowStep
import Formal.Elab.CoreEval
import Formal.Elab.CoreCgHdl

/-!
# λFlick⁺ as a `Target`: the relation `Rel` of the run

The base pairs `DP S root a b` are the dictionaries the two sides compute: `a` is closed, typed
at a pure stage and not yet a value, `b` is closed, and they have the same values (`SemEq`).
`Cg (DP S root) false` relates two threads; `CfgR` relates two configurations: the main
threads, the store pointwise, and the regions pointwise with equal `live` flags and related
children and finalizers.

A step whose redex lies inside the left side of a base pair is a pure step of that side
(`dp_calm`): a term typed at the pure stage has no runtime redex, no operation, no registration and
no `raise`. The pair stays a base pair, and when its left side becomes a value the right side
reaches the same value (`DP.step`).
-/

namespace Flick.Elab

variable {S : Sig} {root : Roots}

theorem Frame.size_lt (F : Frame) (a : Flick.Tm) : sizeOf a < sizeOf (F.plug a) := by
  cases F <;> simp [Frame.plug] <;> omega

/-- A term that is not a value is a redex in an ordered context. -/
theorem decomp_exists (x : Flick.Tm) (hx : ¬ Value x) :
    ∃ E r, OkE E ∧ Redex r ∧ x = Flick.plug E r := by
  by_cases h : ∃ (F : Frame) (a : Flick.Tm), F.ok ∧ F.plug a = x ∧ ¬ Value a
  · obtain ⟨F, a, hF, rfl, ha⟩ := h
    have : sizeOf a < sizeOf (F.plug a) := Frame.size_lt F a
    obtain ⟨E, r, hE, hr, rfl⟩ := decomp_exists a ha
    exact ⟨E ++ [F], r, hE.append hF, hr, (plug_snoc E F r).symm⟩
  · exact ⟨[], x, OkE.nil, ⟨hx, fun F a hF ha => Classical.byContradiction fun hv =>
      h ⟨F, a, hF, ha, hv⟩⟩, rfl⟩
termination_by sizeOf x

/-- **The base pairs**: a closed computation typed at the pure stage that is not yet a value,
and a closed term with the same values. -/
def DP (S : Sig) (root : Roots) (a b : Flick.Tm) : Prop :=
  ¬ Value a ∧ b.Closed ∧ (a.NoIns ∧ b.NoIns ∧ ¬ IsAlts b) ∧ SemEq root a b ∧
    ∃ Θ t, Typ S [] Θ [] a (.ex t .pure)

theorem DP.closed : BClosed (DP S root) := by
  rintro a b ⟨-, hb, -, -, Θ, t, ht⟩
  exact ⟨fun i hi => by have := ht.frees i hi; simp at this, hb⟩

theorem DP.noVal : BNoVal (DP S root) := fun _ _ h => h.1

theorem DP.plain : BPlain (DP S root) := fun _ _ h => h.2.2.1

theorem DP.noRaise : BNoRaise (DP S root) :=
  ⟨by rintro a b ⟨-, -, -, -, Θ, t, ht⟩; exact raise_not_pure ht,
    by rintro p ℓ a b ⟨-, -, -, -, Θ, t, ht⟩; exact unw_not_pure ht⟩

theorem fin_redex {c : Const} {p : Nat} {t : Flick.Tm} (hc : c = .addFin ∨ c = .spawn)
    (ht : Value t) : Redex (.app (.app (.const c) (.own p)) t) := by
  refine ⟨fin_not_value hc, fun F a hF ha => ?_⟩
  cases F <;> simp [Frame.plug] at ha
  · rw [ha.1]; exact .constApp (by rcases hc with rfl | rfl <;> rfl) .own
  · rw [ha.2]; exact ht

theorem raise_redex {v : Flick.Tm} (hv : Value v) : Redex (.raise v) := by
  refine ⟨raise_not_value, fun F a _ ha => ?_⟩
  cases F <;> simp [Frame.plug] at ha
  exact ha ▸ hv

theorem unw_redex (p ℓ : Nat) (a : Flick.Tm) : Redex (.unw p ℓ a) :=
  ⟨fun h => (by cases h), fun F a _ ha => (by cases F <;> simp [Frame.plug] at ha)⟩

theorem perform_redex {E o : Name} {c : OpClass} {v : Flick.Tm} (hv : Value v) :
    Redex (.app (.const (.perform E o c)) v) := by
  refine ⟨perform_not_value, fun F a _ ha => ?_⟩
  cases F <;> simp [Frame.plug] at ha
  · exact ha.1 ▸ .const
  · exact ha.2 ▸ hv

theorem perform_app_not_pure {Ψ Θ E o c v t} (h : Typ S Ψ Θ [] (.app (.const (.perform E o c)) v) (.ex t .pure)) :
    False := by
  cases h with
  | app hf _ hψ _ =>
    cases hf with
    | perform _ hT =>
      obtain ⟨_, _, _, _, he, -, lψ, -, -⟩ := hT.arrow_left
      cases he; exact Stage.le_trans lψ hψ
    | const hc => exact hc.elim
  | errVal hv => exact (perform_not_value hv).elim

theorem kont_redex {p ℓ : Nat} {u : Flick.Tm} (hu : Value u) : Redex (.app (.kont p ℓ) u) := by
  refine ⟨kont_not_value, fun F a _ ha => ?_⟩
  cases F <;> simp [Frame.plug] at ha
  · exact ha.1 ▸ .kont
  · exact ha.2 ▸ hu

/-- A runtime head step is at a redex, or at `raise v` or unwinding under a `try` or an
`inside` frame. -/
theorem _root_.Flick.RHead.redex' {Λ Λ' : List Rgn} {μ μ' : List Flick.Tm} {e e' : Flick.Tm}
    (h : RHead Λ μ e Λ' μ' e') : Redex e ∨
      (∃ (F : Frame) (v : Flick.Tm), F.ok ∧ Value v ∧ e = F.plug (.raise v)) ∨
      ∃ (F : Frame) (q ℓ : Nat) (a : Flick.Tm), F.ok ∧ e = F.plug (.unw q ℓ a) := by
  have nv := h.not_value
  cases h with
  | tryRaise hv => exact .inr (.inl ⟨.try_ _, _, trivial, hv, rfl⟩)
  | hctlRaise _ _ hv => exact .inr (.inl ⟨.hctlA _ _ _ _ _ _ _ _, _, trivial, hv, rfl⟩)
  | hresRaise _ _ hv => exact .inr (.inl ⟨.hresB _ _ _, _, trivial, hv, rfl⟩)
  | hctlUnw _ _ => exact .inr (.inr ⟨.hctlA _ _ _ _ _ _ _ _, _, _, _, trivial, rfl⟩)
  | hresUnw _ _ => exact .inr (.inr ⟨.hresB _ _ _, _, _, _, trivial, rfl⟩)
  | hwithCatch => exact .inr (.inr ⟨.hwith _ _ _ _ _, _, _, _, trivial, rfl⟩)
  | finRun _ _ hw _ _ | close _ _ hw _ _ =>
    rcases hw with hw | ⟨v, hv, rfl⟩ | ⟨q, ℓ, a, rfl⟩
    · left; refine ⟨nv, fun F a _ ha => ?_⟩
      cases F <;> simp [Frame.plug] at ha
      exact ha.2 ▸ hw
    · exact .inr (.inl ⟨.inside _, _, trivial, hv, rfl⟩)
    · exact .inr (.inr ⟨.inside _, _, _, _, trivial, rfl⟩)
  | _ =>
    left; refine ⟨nv, fun F a hF ha => ?_⟩
    cases F <;> simp [Frame.plug] at ha <;>
    first
    | (obtain ⟨rfl, -⟩ := ha
       first
       | assumption | exact .const | exact .task
       | exact .constApp rfl .loc)
    | (obtain ⟨-, rfl⟩ := ha
       first | assumption | exact .loc | exact .cap | exact .own)
    | (subst ha
       first | assumption | exact .task)

/-- The hole of a pure term is typed at the pure stage. -/
theorem hole_pure {Θ : Facts} {t : Flick.Ty} {E : List Frame} {r : Flick.Tm} (hE : OkE E)
    (hr : ¬ Value r) (h : Typ S [] Θ [] (Flick.plug E r) (.ex t .pure)) :
    ∃ Θ' t', Typ S [] Θ' [] r (.ex t' .pure) := by
  obtain ⟨K, Θ', t', φ', hr', rel, -, -⟩ := ctx_typing E hE hr h
  rw [rel.pure rfl] at hr'
  exact ⟨Θ', t', by simpa using hr'⟩

/-- **A step inside a pure term is a pure step of it.** -/
theorem dp_calm {Θ : Facts} {t : Flick.Ty} {x : Flick.Tm} (hx : ¬ Value x)
    (ht : Typ S [] Θ [] x (.ex t .pure)) {E₁ : List Frame} (hE₁ : OkE E₁) {l : Lbl}
    {Λ Λ' : List Rgn} {μ μ' : List Flick.Tm} {t' : Flick.Tm}
    (hs : Flick.Step root l Λ μ (Flick.plug E₁ x) (.cfg Λ' μ' t')) :
    Λ' = Λ ∧ μ' = μ ∧ ∃ x₁, PStep root x x₁ ∧ t' = Flick.plug E₁ x₁ := by
  obtain ⟨E₀, r₀, hE₀, hr₀, rfl⟩ := decomp_exists x hx
  have hE : OkE (E₀ ++ E₁) := fun F hF => (List.mem_append.mp hF).elim (hE₀ F) (hE₁ F)
  have typ : ∀ {E r}, OkE E → Redex r → Flick.plug E r = Flick.plug E₁ (Flick.plug E₀ r₀) →
      E = E₀ ++ E₁ ∧ r = r₀ ∧ ∃ Θ' t', Typ S [] Θ' [] r (.ex t' .pure) := by
    intro E r hE' hr h
    rw [← plug_append] at h
    obtain ⟨rfl, rfl⟩ := decomp_eq hr hr₀ hE' hE h
    exact ⟨rfl, rfl, hole_pure hE₀ hr.1 ht⟩
  generalize hq : Flick.plug E₁ (Flick.plug E₀ r₀) = q at hs
  cases hs with
  | pure hE' hh =>
    obtain ⟨rfl, rfl, -⟩ := typ hE' hh.redex hq.symm
    exact ⟨rfl, rfl, _, .head hE₀ hh, plug_append _ _ _⟩
  | glob hE' hg =>
    obtain ⟨rfl, rfl, -⟩ := typ hE' (glob_redex _) hq.symm
    exact ⟨rfl, rfl, _, .glob hE₀ hg, plug_append _ _ _⟩
  | rt hE' hh =>
    rcases hh.redex' with hr | ⟨F, v, hF, hv, rfl⟩ | ⟨F, q, ℓ, a, hF, rfl⟩
    · obtain ⟨-, -, Θ', t', h⟩ := typ hE' hr hq.symm
      exact (hh.not_pure h).elim
    · obtain ⟨-, -, Θ', t', h⟩ := typ (E := F :: _) (fun G hG => (List.mem_cons.mp hG).elim
        (fun h => h ▸ hF) (hE' G)) (raise_redex hv) hq.symm
      exact (raise_not_pure h).elim
    · obtain ⟨-, -, Θ', t', h⟩ := typ (E := F :: _) (fun G hG => (List.mem_cons.mp hG).elim
        (fun h => h ▸ hF) (hE' G)) (unw_redex q ℓ a) hq.symm
      exact (unw_not_pure h).elim
  | performRoot hE' _ _ _ hv =>
    obtain ⟨-, -, Θ', t', h⟩ := typ hE' (perform_redex hv) hq.symm
    exact (perform_app_not_pure h).elim
  | addFin hE' _ _ hv =>
    obtain ⟨-, -, Θ', t', h⟩ := typ hE' (fin_redex (.inl rfl) hv) hq.symm
    exact (fin_not_pure h (.inl rfl)).elim
  | spawn hE' _ _ hv =>
    obtain ⟨-, -, Θ', t', h⟩ := typ hE' (fin_redex (.inr rfl) hv) hq.symm
    exact (fin_not_pure h (.inr rfl)).elim
  | raise hE' hF _ _ _ hv =>
    obtain ⟨-, -, Θ', t', h⟩ := typ (E := _ :: _) (fun G hG => (List.mem_cons.mp hG).elim
      (fun h => h ▸ hF) (hE' G)) (raise_redex hv) hq.symm
    exact (raise_not_pure h).elim
  | unw hE' hF =>
    obtain ⟨-, -, Θ', t', h⟩ := typ (E := _ :: _) (fun G hG => (List.mem_cons.mp hG).elim
      (fun h => h ▸ hF) (hE' G)) (unw_redex _ _ _) hq.symm
    exact (unw_not_pure h).elim
  | @performAbort E o b v _ _ p ℓ r Ei Ef al hE' hEi _ _ hv =>
    have hq' : Flick.plug (Ei ++ .hwith p ℓ Ef al r :: E) (.app (.const (.perform Ef o .abort)) v) =
        Flick.plug E₁ (Flick.plug E₀ r₀) := by rw [plug_append]; exact hq.symm
    obtain ⟨-, -, Θ', t', h⟩ := typ (E := Ei ++ .hwith p ℓ Ef al r :: E) (hEi.append' (hE'.cons' trivial))
      (perform_redex hv) hq'
    exact (perform_app_not_pure h).elim
  | @performTail E o b v _ _ p ℓ r Ei Ef al hE' hEi _ _ hv =>
    have hq' : Flick.plug (Ei ++ .hwith p ℓ Ef al r :: E) (.app (.const (.perform Ef o .tail)) v) =
        Flick.plug E₁ (Flick.plug E₀ r₀) := by rw [plug_append]; exact hq.symm
    obtain ⟨-, -, Θ', t', h⟩ := typ (E := Ei ++ .hwith p ℓ Ef al r :: E) (hEi.append' (hE'.cons' trivial))
      (perform_redex hv) hq'
    exact (perform_app_not_pure h).elim
  | @performCtl E o b v _ _ p ℓ r Ei Ef al hE' hEi _ _ hv =>
    have hq' : Flick.plug (Ei ++ .hwith p ℓ Ef al r :: E) (.app (.const (.perform Ef o .control)) v) =
        Flick.plug E₁ (Flick.plug E₀ r₀) := by rw [plug_append]; exact hq.symm
    obtain ⟨-, -, Θ', t', h⟩ := typ (E := Ei ++ .hwith p ℓ Ef al r :: E) (hEi.append' (hE'.cons' trivial))
      (perform_redex hv) hq'
    exact (perform_app_not_pure h).elim
  | @resume E A k p u _ _ ℓ r Ef al Ei ℓh hE' hA _ _ _ hu =>
    have hq' : Flick.plug (A ++ .hctlA p ℓh ℓ false Ef al r k :: E) (.app (.kont p ℓ) u) =
        Flick.plug E₁ (Flick.plug E₀ r₀) := by rw [plug_append]; exact hq.symm
    obtain ⟨-, -, Θ', t', h⟩ := typ (E := A ++ .hctlA p ℓh ℓ false Ef al r k :: E)
      (hA.append' (hE'.cons' trivial)) (kont_redex hu) hq'
    cases h with
    | app hf => cases hf with | kont hl => simp at hl
    | errVal hv => exact (kont_not_value hv).elim

/-- A pure step makes no run-time form. -/
theorem _root_.Flick.PStep.noIns {x x₁ : Flick.Tm} (hs : PStep root x x₁) (hroot : RootTy S root) (hn : x.NoIns) :
    x₁.NoIns := by
  obtain ⟨E, e, e', -, -, rfl, rfl, hh⟩ := hs.inv
  intro h
  rcases plug_has_inv (r := e) E h with h | h
  · exact hn h
  · rcases hh with hh | ⟨g, rfl, hg⟩
    · exact hn (plug_has E (hh.has h))
    · exact (hroot.glob _ _ hg).1 _ h

/-- A pure step keeps the typing at the pure stage. -/
theorem pstep_typed (hroot : RootTy S root) {Θ : Facts} {t : Flick.Ty} {x x₁ : Flick.Tm}
    (ht : Typ S [] Θ [] x (.ex t .pure)) (hs : PStep root x x₁) : Typ S [] Θ [] x₁ (.ex t .pure) := by
  obtain ⟨E, e, e', hE, he, rfl, rfl, hh⟩ := hs.inv
  obtain ⟨K, Θ', t', φ', hr, -, -, back⟩ := ctx_typing E hE he.1 ht
  simp only [List.map_nil] at hr
  have := back.id [] e'
  simp only [List.nil_append, List.map_nil] at this
  apply this
  rcases hh with hh | ⟨g, rfl, hg⟩
  · exact pure_preservation hr hh
  · exact glob_preservation hroot hr hg

/-- **A step of the left side of a base pair**: the pair stays a base pair, or its left side is
a value that the right side reaches. -/
theorem DP.step (hroot : RootTy S root) {x x₂ x₁ : Flick.Tm} (h : DP S root x x₂)
    (hs : PStep root x x₁) : (¬ Value x₁ → DP S root x₁ x₂) ∧ (Value x₁ → Eval root x₂ x₁) := by
  obtain ⟨-, hc, ⟨hn, hp⟩, hse, Θ, t, ht⟩ := h
  have hse₁ : SemEq root x₁ x₂ := fun v => (Eval.of_step hs).symm.trans (hse v)
  refine ⟨fun hv => ⟨hv, hc, ⟨hs.noIns hroot hn, hp⟩, hse₁, Θ, t, pstep_typed hroot ht hs⟩,
    fun hv => ?_⟩
  exact (hse₁ x₁).1 (.value hv)

/-- Two regions with one `live` flag and related children and finalizers. -/
def RgR (B : Flick.Tm → Flick.Tm → Prop) (g g' : Rgn) : Prop :=
  g.live = g'.live ∧ All2 (Cg B false) g.kids g'.kids ∧ All2 (Cg B false) g.fins g'.fins

/-- **`Rel`**: related main threads, stores and regions. -/
structure CfgR (B : Flick.Tm → Flick.Tm → Prop) (c c' : Cfg) : Prop where
  main : Cg B false c.main c'.main
  mem : All2 (Cg B false) c.mem c'.mem
  rgs : All2 (RgR B) c.rgs c'.rgs

theorem All2.getElem? {α β : Type} {R : α → β → Prop} {l : List α} {l' : List β} (h : All2 R l l')
    {i : Nat} {a : α} (ha : l[i]? = some a) : ∃ b, l'[i]? = some b ∧ R a b := by
  induction h generalizing i with
  | nil => simp at ha
  | cons r _ ih =>
    cases i with
    | zero => simp at ha; subst ha; exact ⟨_, rfl, r⟩
    | succ i => exact ih (by simpa using ha)

theorem All2.set {α β : Type} {R : α → β → Prop} {l : List α} {l' : List β} (h : All2 R l l')
    (i : Nat) {a : α} {b : β} (hab : R a b) : All2 R (l.set i a) (l'.set i b) := by
  induction h generalizing i with
  | nil => exact .nil
  | cons r _ ih =>
    cases i with
    | zero => exact .cons hab ‹_›
    | succ i => exact .cons r (ih i)

theorem All2.lt {α β : Type} {R : α → β → Prop} {l : List α} {l' : List β} (h : All2 R l l')
    {i : Nat} (hi : i < l.length) : i < l'.length := h.length ▸ hi

theorem All2.mem {α β : Type} {R : α → β → Prop} {l : List α} {l' : List β} (h : All2 R l l')
    {b : β} (hb : b ∈ l') : ∃ a ∈ l, R a b := by
  induction h with
  | nil => simp at hb
  | cons r _ ih =>
    rcases List.mem_cons.mp hb with rfl | hb
    · exact ⟨_, by simp, r⟩
    · obtain ⟨a, ha, h⟩ := ih hb; exact ⟨a, by simp [ha], h⟩

theorem All2.snoc_right {α β : Type} {R : α → β → Prop} {l : List α} {l' : List β} {a : α}
    (h : All2 R l l') {fs : List α} (hl : l = fs ++ [a]) :
    ∃ fs' b, l' = fs' ++ [b] ∧ All2 R fs fs' ∧ R a b := by
  subst hl
  induction fs generalizing l' with
  | nil => cases h with
    | cons r h => cases h; exact ⟨[], _, rfl, .nil, r⟩
  | cons x xs ih => cases h with
    | cons r h =>
      obtain ⟨fs', b, rfl, h₁, h₂⟩ := ih h
      exact ⟨_ :: fs', b, rfl, .cons r h₁, h₂⟩

theorem RgR.setKid {B : Flick.Tm → Flick.Tm → Prop} {Λ Λ' : List Rgn}
    (h : All2 (RgR B) Λ Λ') (p j : Nat) {k k' : Flick.Tm} (hk : Cg B false k k') :
    All2 (RgR B) (Flick.setKid Λ p j k) (Flick.setKid Λ' p j k') := by
  unfold Flick.setKid
  cases hg : Λ[p]? with
  | none =>
    have : Λ'[p]? = none := by
      rw [List.getElem?_eq_none_iff] at hg ⊢; rw [← h.length]; exact hg
    rw [this]; exact h
  | some g =>
    obtain ⟨g', hg', r⟩ := h.getElem? hg
    rw [hg']
    exact h.set p ⟨r.1, r.2.1.set j hk, r.2.2⟩

end Flick.Elab
