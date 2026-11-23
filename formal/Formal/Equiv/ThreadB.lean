import Formal.Equiv.ThreadA

/-!
# Contextual equivalence: steps of a thread that read its context

The focus of a step that reads its context is related at `top` once the right side catches up
(`focus_cases`): no special pair holds it, as their foci are forms of lazy cells (`Act.simple`). A finalizer or a child registered with an owner
(`addFin_sim`, `spawn_sim`), an operation `Σ₀` answers (`root_sim`), a global (`glob_sim`), a raised
error or a signal leaving a frame (`raise_sim`, `unw_sim`): the right side does the same, after it
leaves the frames of its own cells.
-/

namespace Flick.Equiv

open Flick.Elab (All2 IsAlts)

variable {W : World}

/-- A root of source code. -/
def _root_.Flick.Roots.Src (root : Roots) : Prop :=
  (∀ N v, root.svc N = some v → v.Src) ∧ ∀ g v, root.glob g = some v → v.Src

theorem _root_.Flick.Tm.Src.field : ∀ {r : Tm}, r.Src → ∀ {l : Label} {w : Tm}, r.field l = some w →
    w.Src := by
  intro r
  induction r with
  | rcons l' e rest _ ihr =>
    intro h l w hw
    simp only [Tm.field] at hw
    split at hw
    · cases hw; exact h.1
    · exact ihr h.2 hw
  | drec c r ih => intro h l w hw; exact ih h hw
  | _ => intro _ _ _ hw; simp [Tm.field] at hw

/-- The focus of a non-pure step, after the right side catches up. -/
theorem focus_cases {root : Roots} {l : Lbl} {Λ Λ' Λ₂ : List Rgn} {μ μ' μ₂ : List Tm}
    {τ : Option (Nat × Nat)} {E : List Frame} {r t₂ t' : Tm} (hE : OkE E) (hrv : ¬ Value r)
    (hr : ∀ f a, r = .app f a → Value f ∧ Value a) (hrc : ∀ l e a, r ≠ .rcons l e a)
    (hal : ¬ IsAlts r) (hns : ∀ n a, r ≠ .settle n a)
    (ht : Sim W .any (plug E r) t₂) (hst : StR W μ μ₂) (hR : W.R = root) (hcl₁ : (plug E r).Closed)
    (hcl₂ : ∀ μ'' t'', KSteps root μ₂ t₂ μ'' t'' → t''.Closed) (_hrg : RgsR W μ τ Λ Λ₂)
    (hs : Step root l Λ μ (plug E r) (.cfg Λ' μ' t'))
    (hcong : ∀ W' μ₂' E₂ r₂, RJ W μ₂ t₂ W' μ₂' (plug E₂ r₂) → FrL W' E E₂ → OkE E₂ →
      Sim W' .top r r₂ → TRes root W τ Λ₂ μ₂ t₂ μ Λ' μ' t')
    (hsr : ¬ r.ActFocus := by simp [Tm.ActFocus]) (hnp : l ≠ .pure := by intro h; cases h) :
    TRes root W τ Λ₂ μ₂ t₂ μ Λ' μ' t' := by
  rcases ht.dec hrv hr hrc hal
      (fun _ _ he => by subst he; exact (hsr trivial).elim)
      (fun _ _ he => by subst he; exact (hsr trivial).elim)
      _ _ rfl hE μ₂ hst (fun _ => ⟨hcl₁, hcl₂ _ _ .refl⟩) with
    hd | ⟨W', μ₂', E₂, r₂, hj, hfr, hE₂, hs'⟩ |
    ⟨W', μ₂', E₂, n, a, b, hj, hfr, hE₂, rfl, hn, hs'⟩
  · obtain ⟨W', μ₂', E₀, E₁, a, b, E₂, hj, rfl, ha, -, -, hact⟩ := hd
    rcases hact.simple (hj.st _ hst) hrv ha with hsm | ⟨l₀, l', e, t, rfl, hH, hhl, hpt, hct⟩
    · exact (hsr hsm).elim
    · obtain ⟨-, -, -, -, -, hpe, -⟩ := (hj.st _ hst).hl l₀ e hhl
      have hR' : W'.R = root := hj.le.R.symm.trans hR
      have hnv : ¬ Value t := by
        rcases snoc_or E₀ with rfl | ⟨E₀', F, rfl⟩
        · simp only [plug] at ha; exact (hns l₀ t ha.symm).elim
        · rw [plug_snoc] at ha
          cases F <;> simp only [Frame.plug, reduceCtorEq, Tm.settle.injEq] at ha
          exact ha.2 ▸ not_value_plugs hrv E₀'
      rw [plug_append, ← ha] at hs
      have hEq := (PureEval.step_settle (fun G hG => hE G (List.mem_append_right _ hG))
        (hR' ▸ hpe) (hR' ▸ hpt) hnv hs).1
      exact (hnp hEq).elim
  · exact hcong W' μ₂' E₂ r₂ hj hfr hE₂ hs'
  · exact (hns _ _ rfl).elim

/-- The empty context is related only to frames of right cells. -/
theorem FrL.nil_left {A : List Frame} (h : FrL W [] A) : ∀ G ∈ A, ∃ n, G = .settle n ∧ W.J₂ n := by
  generalize he : ([] : List Frame) = E at h
  induction h with
  | nil => intro G hG; cases hG
  | cons => cases he
  | junkL => cases he
  | againF | againR => cases he
  | junkR hn _ ih =>
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · exact ⟨_, rfl, hn⟩
    · exact ih he G hG

/-- A raised error leaves the frames of right cells. -/
theorem RJ.unwind_raise {v : Tm} (hv : Value v) :
    ∀ {A : List Frame} (μ : List Tm), (∀ G ∈ A, ∃ n, G = .settle n ∧ W.J₂ n) →
      ∃ μ', RJ W μ (Flick.plug A (.raise v)) W μ' (.raise v)
  | [], μ, _ => ⟨μ, RJ.refl _ _⟩
  | G :: A, μ, h => by
    obtain ⟨n, rfl, hn⟩ := h G (List.mem_cons_self ..)
    obtain ⟨μ', hj⟩ := RJ.unwind_raise hv (A := A) (settleCell μ n (.raise v))
      (fun G hG => h G (List.mem_cons_of_mem _ hG))
    exact ⟨μ', ((RJ.settleR hn hv).plug (E := A) (fun G hG => by
      obtain ⟨m, rfl, -⟩ := h G (List.mem_cons_of_mem _ hG); trivial)).trans hj⟩

/-- A signal leaves the frames of right cells. -/
theorem RJ.unwind_unw {p ℓ : Nat} {a : Tm} :
    ∀ {A : List Frame} (μ : List Tm), (∀ G ∈ A, ∃ n, G = .settle n ∧ W.J₂ n) →
      ∃ μ', RJ W μ (Flick.plug A (.unw p ℓ a)) W μ' (.unw p ℓ a)
  | [], μ, _ => ⟨μ, RJ.refl _ _⟩
  | G :: A, μ, h => by
    obtain ⟨n, rfl, hn⟩ := h G (List.mem_cons_self ..)
    obtain ⟨μ', hj⟩ := RJ.unwind_unw (A := A) (resetCell μ n)
      (fun G hG => h G (List.mem_cons_of_mem _ hG))
    exact ⟨μ', ((RJ.settleU hn).plug (E := A) (fun G hG => by
      obtain ⟨m, rfl, -⟩ := h G (List.mem_cons_of_mem _ hG); trivial)).trans hj⟩

theorem RgsR.get {μ : List Tm} {τ : Option (Nat × Nat)} {Λ Λ₂ : List Rgn} (h : RgsR W μ τ Λ Λ₂)
    {p : Nat} {g : Rgn} (hg : Λ[p]? = some g) : ∃ g₂, Λ₂[p]? = some g₂ ∧ RgR W μ τ p g g₂ := by
  have hp : p < Λ₂.length := h.1 ▸ (List.getElem?_eq_some_iff.mp hg).1
  obtain ⟨g₂, hg₂⟩ : ∃ g₂, Λ₂[p]? = some g₂ := ⟨_, List.getElem?_eq_getElem hp⟩
  exact ⟨g₂, hg₂, h.2 p g g₂ hg hg₂⟩

end Flick.Equiv
