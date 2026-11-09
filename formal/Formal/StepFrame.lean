import Formal.Lemma3

/-!
# Steps under one more frame
-/

namespace Flick

variable {S : Sig}

/-- A region that is not live: closed or never created. -/
def Dead (Λ : List Rgn) (p : Nat) : Prop := ∀ g, Λ[p]? = some g → g.live = false

/-- A redex that does not step: a finished region body that waits for a child,
or a step that needs a region that is not live. -/
def Blocks (Λ : List Rgn) (r : Tm) : Prop :=
  (∃ p w g, r = .inside p w ∧ Final w ∧ Λ[p]? = some g ∧ g.live = true ∧ ∃ k ∈ g.kids, ¬ Final k) ∨
    ∃ p, Dead Λ p ∧ (r = .app (.const .open_) (.own p) ∨ r = .app (.const .op) (.cap p) ∨
      (∃ t, r = .app (.app (.const .addFin) (.own p)) t) ∨
      (∃ t, r = .app (.app (.const .spawn) (.own p)) t) ∨ (∃ w, r = .inside p w ∧ Final w) ∨
      ∃ ℓ Ef al Ei o v, r = .hkeep p ℓ Ef al (plug Ei (.app (.const (.perform Ef o .tail)) v)) ∧
        OkE Ei ∧ ∀ G ∈ Ei, G.handles Ef o .tail = none)

/-- A term whose redex blocks. -/
def Blocked (Λ : List Rgn) (e : Tm) : Prop := ∃ E r, OkE E ∧ e = plug E r ∧ Blocks Λ r

/-- `Σ₀` answers an operation: it is `tail`, and its root has a function for it. -/
def RootAnswers (root : Roots) (Ef o : Name) (c : OpClass) : Prop :=
  c = .tail ∧ ∃ w f, root.svc Ef = some w ∧ w.field o = some f

/-- An operation of a declared effect that no handler of its context and no root answers; `D`
must hold its effect. -/
def PerformStuck (S : Sig) (root : Roots) (e : Tm) (D : Row) : Prop :=
  ∃ E Ef o c v a b, OkE E ∧ Value v ∧ e = plug E (.app (.const (.perform Ef o c)) v) ∧
    S.op Ef o = some (c, a, b) ∧ (∀ F ∈ E, F.handles Ef o c = none) ∧ ¬ RootAnswers root Ef o c ∧
    D (.nm Ef)

/-- The non-value outcomes of Progress: `raise v`, a step, an operation nothing answers, a
redex that blocks, or unwinding to a handler. -/
inductive NV (S : Sig) (root : Roots) (Λ : List Rgn) (μ : List Tm) (e : Tm) (D : Row) : Prop
  | raise : (∃ v, Value v ∧ e = .raise v) → NV S root Λ μ e D
  | step : (∃ l o, Step root l Λ μ e o) → NV S root Λ μ e D
  | blocked : Blocked Λ e → NV S root Λ μ e D
  | perform : PerformStuck S root e D → NV S root Λ μ e D
  | unw : (∃ p ℓ a, e = .unw p ℓ a) → NV S root Λ μ e D

theorem Step.ofP {root Λ μ e e'} (h : PHead e e') : Step root .pure Λ μ e (.cfg Λ μ e') :=
  Step.pure (E := []) OkE.nil h

theorem Step.ofR {root Λ μ e Λ' μ' e'} (h : RHead Λ μ e Λ' μ' e') :
    Step root .rt Λ μ e (.cfg Λ' μ' e') :=
  Step.rt (E := []) OkE.nil h

theorem Step.snoc {root l Λ μ E x o} {F : Frame} (h : Step root l Λ μ (plug (E ++ [F]) x) o) :
    Step root l Λ μ (F.plug (plug E x)) o := by
  rwa [plug_append] at h

/-- A context with one more frame around it: the frame at its end, or one of its frames. -/
theorem snoc_eq_mid {E A E' : List Frame} {F X : Frame} (h : E ++ [F] = A ++ X :: E') :
    (E' = [] ∧ E = A ∧ F = X) ∨ ∃ E'', E' = E'' ++ [F] ∧ E = A ++ X :: E'' := by
  rcases List.eq_nil_or_concat E' with rfl | ⟨E'', G, rfl⟩
  · obtain ⟨h₁, h₂⟩ := List.append_inj' h rfl
    exact .inl ⟨rfl, h₁, by simpa using h₂⟩
  · have h' : E ++ [F] = (A ++ X :: E'') ++ [G] := by simpa using h
    obtain ⟨h₁, h₂⟩ := List.append_inj' h' rfl
    simp only [List.cons.injEq, and_true] at h₂
    exact .inr ⟨E'', by rw [h₂]; simp, h₁⟩

theorem live_or_dead (Λ : List Rgn) (p : Nat) :
    (∃ g, Λ[p]? = some g ∧ g.live = true) ∨ Dead Λ p := by
  cases hg : Λ[p]? with
  | none => exact .inr fun g h => by rw [hg] at h; cases h
  | some g =>
    cases hl : g.live with
    | true => exact .inl ⟨g, rfl, hl⟩
    | false => exact .inr fun g' h => by rw [hg] at h; cases h; exact hl

/-- A `tail` operation a kept handler answers steps while its region is live. -/
theorem kept_answer {root Λ μ q ℓ Ef al E o v b} (hE : OkE E)
    (hh : ∀ G ∈ E, G.handles Ef o .tail = none) (hb : al.armOf o = some b) (hv : Value v) :
    (∃ l' o', Step root l' Λ μ (.hkeep q ℓ Ef al (plug E (.app (.const (.perform Ef o .tail)) v))) o') ∨
      Blocked Λ (.hkeep q ℓ Ef al (plug E (.app (.const (.perform Ef o .tail)) v))) := by
  rcases live_or_dead Λ q with ⟨g, hg, hl⟩ | hd
  · exact .inl ⟨_, _, Step.performKept (E := []) OkE.nil hE hh hb hv ⟨g, hg, hl⟩⟩
  · exact .inr ⟨[], _, OkE.nil, rfl, .inr ⟨q, hd, .inr (.inr (.inr (.inr (.inr
      ⟨ℓ, Ef, al, E, o, v, rfl, hE, hh⟩))))⟩⟩

/-- An operation of `Σ₀` under one more frame: that frame may be its handler. -/
theorem Step.frame_root {root Λ μ E Ef o v w f} {F : Frame} (hF : F.ok) (hE : OkE E)
    (hh : ∀ G ∈ E, G.handles Ef o .tail = none) (hr : root.svc Ef = some w) (hf : w.field o = some f)
    (hv : Value v) :
    (∃ l' o', Step root l' Λ μ (F.plug (plug E (.app (.const (.perform Ef o .tail)) v))) o') ∨
      Blocked Λ (F.plug (plug E (.app (.const (.perform Ef o .tail)) v))) := by
  cases hF' : F.handles Ef o .tail with
  | none =>
    have := Step.performRoot (root := root) (Λ := Λ) (μ := μ) (hE.append hF) (fun G hG => by
      rcases List.mem_append.mp hG with hG | hG
      · exact hh G hG
      · simp at hG; subst hG; exact hF') hr hf hv
    rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
  | some p =>
    cases F with
    | hwith p' ℓ' E' al r =>
      simp only [Frame.handles] at hF'
      split at hF'
      · rename_i hc
        obtain ⟨rfl, hs⟩ := hc
        obtain ⟨b', hb'⟩ := Option.isSome_iff_exists.mp hs
        exact .inl ⟨_, _, Step.performTail (E := []) OkE.nil hE hh hb' hv⟩
      · cases hF'
    | hkeep q' ℓ' E' al =>
      simp only [Frame.handles] at hF'
      split at hF'
      · rename_i hc
        obtain ⟨-, rfl, hs⟩ := hc
        obtain ⟨b', hb'⟩ := Option.isSome_iff_exists.mp hs
        exact kept_answer hE hh hb' hv
      · cases hF'
    | _ => simp [Frame.handles] at hF'

theorem Step.frame {root l Λ μ e o} (h : Step root l Λ μ e o) {F : Frame} (hF : F.ok)
    (hk : F.Parks := by trivial) : (∃ l' o', Step root l' Λ μ (F.plug e) o') ∨ Blocked Λ (F.plug e) := by
  cases h with
  | pure hE hh =>
    have := Step.pure (root := root) (Λ := Λ) (μ := μ) (hE.append hF) hh
    rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
  | rt hE hh =>
    have := Step.rt (root := root) (hE.append hF) hh
    rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
  | performRoot hE hh hr hf hv => exact Step.frame_root hF hE hh hr hf hv
  | raise hE hG hnt hni hnc hv =>
    have := Step.raise (root := root) (Λ := Λ) (μ := μ) (hE.append hF) hG hnt hni hnc hv
    rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
  | unw hE hG hni hnc hnh =>
    exact .inl ⟨_, _, Step.snoc (Step.unw (hE.append hF) hG hni hnc hnh)⟩
  | performAbort hE hEi hh ha hv =>
    exact .inl ⟨_, _, Step.snoc (Step.performAbort (hE.append hF) hEi hh ha hv)⟩
  | performTail hE hEi hh ha hv =>
    exact .inl ⟨_, _, Step.snoc (Step.performTail (hE.append hF) hEi hh ha hv)⟩
  | performKept hE hEi hh ha hv hl =>
    exact .inl ⟨_, _, Step.snoc (Step.performKept (hE.append hF) hEi hh ha hv hl)⟩
  | performCtl hE hEi hh ha hv =>
    exact .inl ⟨_, _, Step.snoc (Step.performCtl (hE.append hF) hEi hh ha hv)⟩
  | resume hE hA hEi hk' hn hu =>
    exact .inl ⟨_, _, Step.snoc (Step.resume (hE.append hF) hA hEi hk' hn hu)⟩
  | @resumeDie E u p ℓ _ _ hE hu hn =>
    by_cases hR : Resumable (E ++ [F]) p ℓ
    · obtain ⟨A, E', ℓh, Ef, al, r, k, hEq, hA⟩ := hR
      rcases snoc_eq_mid hEq with ⟨rfl, rfl, rfl⟩ | ⟨E'', rfl, rfl⟩
      · obtain ⟨Ei, hEi, rfl⟩ := hk
        exact .inl ⟨_, _, Step.resume (E := []) OkE.nil hE hEi rfl hA hu⟩
      · exact (hn ⟨A, E'', ℓh, Ef, al, r, k, rfl, hA⟩).elim
    · exact .inl ⟨_, _, Step.snoc (Step.resumeDie (hE.append hF) hu hR)⟩
  | todo hE =>
    have := Step.todo (root := root) (Λ := Λ) (μ := μ) (hE.append hF)
    rw [plug_append] at this; exact .inl ⟨_, _, this⟩
  | glob hE hg =>
    have := Step.glob (root := root) (Λ := Λ) (μ := μ) (hE.append hF) hg
    rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
  | addFin hE hg hl hv =>
    have := Step.addFin (root := root) (μ := μ) (hE.append hF) hg hl hv
    rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
  | spawn hE hg hl hv =>
    have := Step.spawn (root := root) (μ := μ) (hE.append hF) hg hl hv
    rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩

theorem Blocked.frame {Λ e} (h : Blocked Λ e) {F : Frame} (hF : F.ok) : Blocked Λ (F.plug e) := by
  obtain ⟨E, r, hE, rfl, hb⟩ := h
  exact ⟨E ++ [F], r, hE.append hF, by rw [plug_append]; rfl, hb⟩

/-- An operation the frame around handles steps; one it does not stays stuck. -/
theorem PerformStuck.frame {root Λ μ e D D'} (h : PerformStuck S root e D) {F : Frame} (hF : F.ok)
    (hD : ∀ N : Name, D (.nm N) → F.dom (.nm N) ∨ D' (.nm N))
    (hpart : ∀ N o c, (∃ a b, S.op N o = some (c, a, b)) → F.installs N → F.handles N o c = none →
      D (.nm N) → D' (.nm N)) :
    (∃ l o, Step root l Λ μ (F.plug e) o) ∨ PerformStuck S root (F.plug e) D' ∨ Blocked Λ (F.plug e) := by
  obtain ⟨E, Ef, o, c, v, a, b, hE, hv, rfl, hop, hh, hna, hN⟩ := h
  cases hF' : F.handles Ef o c with
  | none =>
    refine .inr (.inl ⟨E ++ [F], Ef, o, c, v, a, b, hE.append hF, hv, by rw [plug_append]; rfl, hop,
      fun G hG => ?_, hna, ?_⟩)
    · rcases List.mem_append.mp hG with hG | hG
      · exact hh G hG
      · simp at hG; subst hG; exact hF'
    · rcases hD _ hN with h | h
      · exact hpart Ef o c ⟨a, b, hop⟩ h hF' hN
      · exact h
  | some p =>
    cases F with
    | hwith p' ℓ' E' al r =>
      simp only [Frame.handles] at hF'
      split at hF'
      · rename_i hc
        cases hF'
        obtain ⟨rfl, hs⟩ := hc
        obtain ⟨b', hb'⟩ := Option.isSome_iff_exists.mp hs
        cases c with
        | abort => exact .inl ⟨_, _, Step.performAbort (E := []) OkE.nil hE hh hb' hv⟩
        | tail => exact .inl ⟨_, _, Step.performTail (E := []) OkE.nil hE hh hb' hv⟩
        | control => exact .inl ⟨_, _, Step.performCtl (E := []) OkE.nil hE hh hb' hv⟩
      · cases hF'
    | hkeep q' ℓ' E' al =>
      simp only [Frame.handles] at hF'
      split at hF'
      · rename_i hc
        obtain ⟨rfl, rfl, hs⟩ := hc
        obtain ⟨b', hb'⟩ := Option.isSome_iff_exists.mp hs
        exact (kept_answer hE hh hb' hv).imp id .inr
      · cases hF'
    | _ => simp [Frame.handles] at hF'

/-- Lift the non-value outcomes over a frame that is neither `try` nor `inside`. -/
theorem NV.frame {root Λ μ e D D'} (h : NV S root Λ μ e D) {F : Frame} (hF : F.ok)
    (hnt : ¬ F.isTry) (hni : ¬ F.isInside) (hD : ∀ N : Name, D (.nm N) → F.dom (.nm N) ∨ D' (.nm N))
    (hk : F.Parks := by trivial)
    (hpart : ∀ N o c, (∃ a b, S.op N o = some (c, a, b)) → F.installs N →
      F.handles N o c = none → D (.nm N) → D' (.nm N) := by
      intro N o _ _ h; simp [Frame.installs] at h) :
    NV S root Λ μ (F.plug e) D' := by
  cases h with
  | raise h =>
    obtain ⟨v, hv, rfl⟩ := h
    by_cases hc : F.isCtl
    · cases F <;> simp only [Frame.isCtl] at hc
      · obtain ⟨Ei, hEi, hk⟩ := hk
        exact .step ⟨_, _, Step.ofR (.hctlRaise hEi hk hv)⟩
      · obtain ⟨A, hA, hk⟩ := hk
        exact .step ⟨_, _, Step.ofR (.hresRaise hA hk hv)⟩
      · obtain ⟨Ei, hEi, hk⟩ := hk
        exact .step ⟨_, _, Step.ofR (.hkarmRaise hEi hk hv)⟩
    · exact .step ⟨_, _, Step.raise (E := []) OkE.nil hF hnt hni hc hv⟩
  | step h =>
    obtain ⟨_, _, hs⟩ := h
    rcases hs.frame hF hk with h | h
    · exact .step h
    · exact .blocked h
  | blocked hb => exact .blocked (hb.frame hF)
  | perform hp =>
    rcases hp.frame (root := root) (Λ := Λ) (μ := μ) hF hD hpart with h | h | h
    · exact .step h
    · exact .perform h
    · exact .blocked h
  | unw h =>
    obtain ⟨p, ℓ, a, rfl⟩ := h
    by_cases hc : F.isCtl
    · cases F <;> simp only [Frame.isCtl] at hc
      · obtain ⟨Ei, hEi, hk⟩ := hk
        exact .step ⟨_, _, Step.ofR (.hctlUnw hEi hk)⟩
      · obtain ⟨A, hA, hk⟩ := hk
        exact .step ⟨_, _, Step.ofR (.hresUnw hA hk)⟩
      · obtain ⟨Ei, hEi, hk⟩ := hk
        exact .step ⟨_, _, Step.ofR (.hkarmUnw hEi hk)⟩
    · by_cases hh : F.catches p ℓ
      · cases F <;> simp only [Frame.catches] at hh
        obtain ⟨rfl, rfl⟩ := hh; exact .step ⟨_, _, Step.ofR .hwithCatch⟩
      · exact .step ⟨_, _, Step.unw (E := []) OkE.nil hF hni hc hh⟩

/-- The common case: the frame keeps the stage. -/
theorem NV.frame' {root Λ μ e D} (h : NV S root Λ μ e D) {F : Frame} (hF : F.ok)
    (hnt : ¬ F.isTry) (hni : ¬ F.isInside) (hk : F.Parks := by trivial) : NV S root Λ μ (F.plug e) D :=
  h.frame hF hnt hni (fun _ h => .inr h) hk (fun _ _ _ _ _ _ h => h)

end Flick
