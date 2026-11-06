import Formal.Lemma3

/-!
# Steps under one more frame
-/

namespace Flick

variable {S : Sig}

/-- `ask_S` whose service is in neither a `with` frame nor `Σ₀`; `D` must hold `S`. -/
def AskStuck (root : Roots) (e : Tm) (D : Row) : Prop :=
  ∃ E N, OkE E ∧ e = plug E (.app (.const (.ask N)) (.lit .unit)) ∧ lookupE E N = none ∧
    root.svc N = none ∧ D (.nm N)

/-- A region that is not live: closed or never created. -/
def Dead (Λ : List Rgn) (p : Nat) : Prop := ∀ g, Λ[p]? = some g → g.live = false

/-- A redex that does not step: a finished region body that waits for a child,
or a step that needs a region that is not live. -/
def Blocks (Λ : List Rgn) (r : Tm) : Prop :=
  (∃ p w g, r = .inside p w ∧ Final w ∧ Λ[p]? = some g ∧ g.live = true ∧ ∃ k ∈ g.kids, ¬ Final k) ∨
    ∃ p, Dead Λ p ∧ (r = .app (.const .open_) (.own p) ∨ r = .app (.const .op) (.cap p) ∨
      (∃ t, r = .app (.app (.const .addFin) (.own p)) t) ∨
      (∃ t, r = .app (.app (.const .spawn) (.own p)) t) ∨ ∃ w, r = .inside p w ∧ Final w)

/-- `ask_S` answered by a `with` frame of a `provide` region that is not live:
the use of a service after its `provide` ended (regions.md §3.3, T4.3). -/
def DeadAsk (Λ : List Rgn) (e : Tm) : Prop :=
  ∃ E N q v, OkE E ∧ e = plug E (.app (.const (.ask N)) (.lit .unit)) ∧ lookupE E N = some (q, v) ∧
    ¬ Live Λ q

/-- A term whose redex blocks, or that asks a service of an ended `provide`. -/
def Blocked (Λ : List Rgn) (e : Tm) : Prop := (∃ E r, OkE E ∧ e = plug E r ∧ Blocks Λ r) ∨ DeadAsk Λ e

/-- The non-value outcomes of Progress. -/
def NV (root : Roots) (Λ : List Rgn) (μ : List Tm) (e : Tm) (D : Row) : Prop :=
  (∃ v, Value v ∧ e = .raise v) ∨ (∃ l o, Step root l Λ μ e o) ∨ AskStuck root e D ∨ Blocked Λ e

theorem Step.ofP {root Λ μ e e'} (h : PHead e e') : Step root .pure Λ μ e (.cfg Λ μ e') :=
  Step.pure (E := []) OkE.nil h

theorem Step.ofR {root Λ μ e Λ' μ' e'} (h : RHead Λ μ e Λ' μ' e') :
    Step root .rt Λ μ e (.cfg Λ' μ' e') :=
  Step.rt (E := []) OkE.nil h

theorem Step.snoc {root l Λ μ E x o} {F : Frame} (h : Step root l Λ μ (plug (E ++ [F]) x) o) :
    Step root l Λ μ (F.plug (plug E x)) o := by
  rwa [plug_append] at h

theorem DeadAsk.frame {Λ e} (h : DeadAsk Λ e) {F : Frame} (hF : F.ok) : DeadAsk Λ (F.plug e) := by
  obtain ⟨E, N, q, v, hE, rfl, hl, hq⟩ := h
  exact ⟨E ++ [F], N, q, v, hE.append hF, by rw [plug_append]; rfl, by rw [lookupE_append, hl], hq⟩

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

theorem Step.frame {root l Λ μ e o} (h : Step root l Λ μ e o) {F : Frame} (hF : F.ok)
    (hk : F.Parks := by trivial) :
    (∃ l' o', Step root l' Λ μ (F.plug e) o') ∨ DeadAsk Λ (F.plug e) := by
  cases h with
  | pure hE hh =>
    have := Step.pure (root := root) (Λ := Λ) (μ := μ) (hE.append hF) hh
    rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
  | rt hE hh =>
    have := Step.rt (root := root) (hE.append hF) hh
    rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
  | askWith hE hl hq =>
    have := Step.askWith (root := root) (Λ := Λ) (μ := μ) (hE.append hF)
      (by rw [lookupE_append, hl]) hq
    rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
  | @askRoot E N v _ _ hE hl hr =>
    cases hF' : F.lookup N with
    | some w =>
      by_cases hq : Live Λ w.1
      · have := Step.askWith (root := root) (Λ := Λ) (μ := μ) (q := w.1) (v := w.2) (hE.append hF)
          (by rw [lookupE_append, hl]; exact hF') hq
        rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
      · exact .inr ⟨E ++ [F], N, w.1, w.2, hE.append hF, by rw [plug_append]; rfl,
          by rw [lookupE_append, hl]; exact hF', hq⟩
    | none =>
      have := Step.askRoot (root := root) (Λ := Λ) (μ := μ) (hE.append hF)
        (by rw [lookupE_append, hl]; exact hF') hr
      rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
  | raise hE hG hnt hni hnc hv =>
    have := Step.raise (root := root) (Λ := Λ) (μ := μ) (hE.append hF) hG hnt hni hnc hv
    rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
  | unw hE hG hni hnc hnh =>
    have := Step.unw (root := root) (Λ := Λ) (μ := μ) (hE.append hF) hG hni hnc hnh
    rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
  | performAbort hE hEi hh ha hv =>
    exact .inl ⟨_, _, Step.snoc (Step.performAbort (hE.append hF) hEi hh ha hv)⟩
  | performTail hE hEi hh ha hv =>
    exact .inl ⟨_, _, Step.snoc (Step.performTail (hE.append hF) hEi hh ha hv)⟩
  | performCtl hE hEi hh ha hv =>
    exact .inl ⟨_, _, Step.snoc (Step.performCtl (hE.append hF) hEi hh ha hv)⟩
  | resume hE hA hEi hk' hn hu =>
    exact .inl ⟨_, _, Step.snoc (Step.resume (hE.append hF) hA hEi hk' hn hu)⟩
  | @resumeDie E u p ℓ _ _ hE hu hn =>
    by_cases hR : Resumable (E ++ [F]) p ℓ
    · obtain ⟨A, E', Ef, al, r, k, hEq, hA⟩ := hR
      rcases snoc_eq_mid hEq with ⟨rfl, rfl, rfl⟩ | ⟨E'', rfl, rfl⟩
      · obtain ⟨Ei, hEi, rfl⟩ := hk
        exact .inl ⟨_, _, Step.resume (E := []) OkE.nil hE hEi rfl hA hu⟩
      · exact (hn ⟨A, E'', Ef, al, r, k, rfl, hA⟩).elim
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

theorem AskStuck.frame {root Λ μ e D D'} (h : AskStuck root e D) {F : Frame} (hF : F.ok)
    (hD : ∀ n, D n → F.dom n ∨ D' n) :
    (∃ l o, Step root l Λ μ (F.plug e) o) ∨ AskStuck root (F.plug e) D' ∨ DeadAsk Λ (F.plug e) := by
  obtain ⟨E, N, hE, rfl, hl, hr, hN⟩ := h
  cases hF' : F.lookup N with
  | some w =>
    by_cases hq : Live Λ w.1
    · have := Step.askWith (root := root) (Λ := Λ) (μ := μ) (q := w.1) (v := w.2) (hE.append hF)
        (by rw [lookupE_append, hl]; exact hF') hq
      rw [plug_append, plug_append] at this; exact .inl ⟨_, _, this⟩
    · exact .inr (.inr ⟨E ++ [F], N, w.1, w.2, hE.append hF, by rw [plug_append]; rfl,
        by rw [lookupE_append, hl]; exact hF', hq⟩)
  | none =>
    refine .inr (.inl ⟨E ++ [F], N, hE.append hF, by rw [plug_append]; rfl,
      by rw [lookupE_append, hl]; exact hF', hr, ?_⟩)
    rcases hD _ hN with h | h
    · simp [Frame.dom, hF'] at h
    · exact h

theorem Blocked.frame {Λ e} (h : Blocked Λ e) {F : Frame} (hF : F.ok) : Blocked Λ (F.plug e) := by
  rcases h with ⟨E, r, hE, rfl, hb⟩ | h
  · exact .inl ⟨E ++ [F], r, hE.append hF, by rw [plug_append]; rfl, hb⟩
  · exact .inr (h.frame hF)

/-- Lift the non-value outcomes over a frame that is neither `try` nor `inside`. -/
theorem NV.frame {root Λ μ e D D'} (h : NV root Λ μ e D) {F : Frame} (hF : F.ok)
    (hnt : ¬ F.isTry) (hni : ¬ F.isInside) (hD : ∀ n, D n → F.dom n ∨ D' n)
    (hk : F.Parks := by trivial) :
    NV root Λ μ (F.plug e) D' := by
  rcases h with ⟨v, hv, rfl⟩ | ⟨_, _, hs⟩ | hs | hb
  · by_cases hc : F.isCtl
    · cases F <;> simp only [Frame.isCtl] at hc
      · obtain ⟨Ei, hEi, hk⟩ := hk
        exact .inr (.inl ⟨_, _, Step.ofR (.hctlRaise hEi hk hv)⟩)
      · obtain ⟨A, hA, hk⟩ := hk
        exact .inr (.inl ⟨_, _, Step.ofR (.hresRaise hA hk hv)⟩)
    · exact .inr (.inl ⟨_, _, Step.raise (E := []) OkE.nil hF hnt hni hc hv⟩)
  · rcases hs.frame hF hk with h | h
    · exact .inr (.inl h)
    · exact .inr (.inr (.inr (.inr h)))
  · rcases hs.frame (Λ := Λ) (μ := μ) hF hD with h | h | h
    · exact .inr (.inl h)
    · exact .inr (.inr (.inl h))
    · exact .inr (.inr (.inr (.inr h)))
  · exact .inr (.inr (.inr (hb.frame hF)))

/-- The common case: the frame keeps the stage. -/
theorem NV.frame' {root Λ μ e D} (h : NV root Λ μ e D) {F : Frame} (hF : F.ok)
    (hnt : ¬ F.isTry) (hni : ¬ F.isInside) (hk : F.Parks := by trivial) : NV root Λ μ (F.plug e) D :=
  h.frame hF hnt hni (fun _ h => .inr h) hk

end Flick
