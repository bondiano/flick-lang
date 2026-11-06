import Formal.Decomp
import Formal.SubstComp
import Formal.Elab.CoreT6a
import Formal.Elab.CoreDSubst

/-!
# λFlick⁺ as a `Target`: evaluation by pure steps

`Eval root a v`: `a` reaches the value `v` by pure steps of one thread (`PStep`), which do not
read the store or the regions. Pure steps are deterministic, so a term has at most one value
(`Eval.det`), and a step keeps it (`Eval.of_step`). A frame around a term is evaluated after
the term (`eval_frame`). `SemEq a b`: `a` and `b` have the same values.

`appsC f args` applies `f` to `args`; `λ^n. D` applied to `n` evaluated arguments steps to
`D` with the values (`beta_chain`), the innermost `λ` taking the first argument.
-/

namespace Flick.Elab

/-- `a` evaluates to the value `v` by pure steps. -/
def Eval (root : Roots) (a v : Flick.Tm) : Prop := Star (PStep root) a v ∧ Value v

/-- `a` and `b` have the same values. -/
def SemEq (root : Roots) (a b : Flick.Tm) : Prop := ∀ v, Eval root a v ↔ Eval root b v

variable {root : Roots}

theorem SemEq.refl (a : Flick.Tm) : SemEq root a a := fun _ => Iff.rfl
theorem SemEq.symm {a b : Flick.Tm} (h : SemEq root a b) : SemEq root b a := fun v => (h v).symm
theorem SemEq.trans {a b c : Flick.Tm} (h₁ : SemEq root a b) (h₂ : SemEq root b c) :
    SemEq root a c := fun v => (h₁ v).trans (h₂ v)

theorem Eval.value {v : Flick.Tm} (h : Value v) : Eval root v v := ⟨.refl, h⟩

theorem Star.value {a b : Flick.Tm} (h : Star (PStep root) a b) (ha : Value a) : a = b := by
  cases h with
  | refl => rfl
  | step s _ => exact absurd ha s.not_value

theorem Eval.det {a v w : Flick.Tm} (h₁ : Eval root a v) (h₂ : Eval root a w) : v = w := by
  obtain ⟨s₁, hv⟩ := h₁
  obtain ⟨s₂, hw⟩ := h₂
  induction s₁ with
  | refl => exact s₂.value hv
  | step s _ ih =>
    cases s₂ with
    | refl => exact absurd hw s.not_value
    | step s' r => cases s.det s'; exact ih hv r

theorem Eval.of_step {a b v : Flick.Tm} (h : PStep root a b) : Eval root a v ↔ Eval root b v := by
  constructor
  · rintro ⟨s, hv⟩
    cases s with
    | refl => exact absurd hv h.not_value
    | step s r => cases h.det s; exact ⟨r, hv⟩
  · rintro ⟨s, hv⟩; exact ⟨.step h s, hv⟩

theorem Eval.of_star {a b v : Flick.Tm} (h : Star (PStep root) a b) :
    Eval root a v ↔ Eval root b v := by
  induction h with
  | refl => exact Iff.rfl
  | step s _ ih => exact (Eval.of_step s).trans ih

theorem SemEq.of_star {a b : Flick.Tm} (h : Star (PStep root) a b) : SemEq root a b :=
  fun _ => Eval.of_star h

theorem SemEq.of_eval {a v : Flick.Tm} (h : Eval root a v) : SemEq root a v :=
  fun _ => (Eval.of_star h.1).trans ⟨fun h' => h', fun h' => h'⟩

theorem Star.frame {a b : Flick.Tm} (h : Star (PStep root) a b) {F : Frame} (hF : F.ok) :
    Star (PStep root) (F.plug a) (F.plug b) := by
  induction h with
  | refl => exact .refl
  | step s _ ih => exact .step (s.frame hF) ih

/-- A frame is evaluated after its hole. -/
theorem eval_frame {F : Frame} (hF : F.ok) {a v : Flick.Tm} :
    Eval root (F.plug a) v ↔ ∃ w, Eval root a w ∧ Eval root (F.plug w) v := by
  constructor
  · rintro ⟨s, hv⟩
    generalize hx : F.plug a = x at s
    induction s generalizing a with
    | refl =>
      subst hx
      by_cases ha : Value a
      · exact ⟨a, .value ha, .value hv⟩
      · exact absurd hv (not_value_plug ha F)
    | step st r ih =>
      subst hx
      by_cases ha : Value a
      · exact ⟨a, .value ha, ⟨.step st r, hv⟩⟩
      · obtain ⟨a', sa, rfl⟩ := st.inv_frame hF ha
        obtain ⟨w, ⟨sw, hw⟩, h⟩ := ih hv rfl
        exact ⟨w, ⟨.step sa sw, hw⟩, h⟩
  · rintro ⟨w, ⟨s, -⟩, h⟩
    exact (Eval.of_star (s.frame hF)).2 h

/-- `f a₁ … aₙ` in the core. -/
def appsC (f : Flick.Tm) (args : List Flick.Tm) : Flick.Tm := args.foldl .app f

theorem _root_.Flick.PStep.apps {a b : Flick.Tm} (h : PStep root a b) (args : List Flick.Tm) :
    PStep root (appsC a args) (appsC b args) := by
  induction args generalizing a b with
  | nil => exact h
  | cons x xs ih => exact ih (h.frame (F := .appL x) trivial)

theorem Star.apps {a b : Flick.Tm} (h : Star (PStep root) a b) (args : List Flick.Tm) :
    Star (PStep root) (appsC a args) (appsC b args) := by
  induction h with
  | refl => exact .refl
  | step s _ ih => exact .step (s.apps args) ih

theorem All2.length {α β : Type} {R : α → β → Prop} {l : List α} {l' : List β} (h : All2 R l l') :
    l.length = l'.length := by
  induction h with
  | nil => rfl
  | cons _ _ ih => simp [ih]


theorem inst_snoc (ws : List Flick.Tm) (v : Flick.Tm) (i : Nat) :
    (liftS ws.length (Flick.inst [v]) i).subst (Flick.inst ws) = Flick.inst (ws ++ [v]) i := by
  unfold liftS
  by_cases h : i < ws.length
  · rw [if_pos h]; simp only [Flick.Tm.subst, Flick.inst]
    rw [dif_pos h, dif_pos (by simp; omega)]
    exact (List.getElem_append_left h).symm
  · rw [if_neg h, Flick.Tm.subst_rename]
    by_cases h' : i = ws.length
    · subst h'
      have e : Flick.inst (ws ++ [v]) ws.length = v := by simp [Flick.inst]
      rw [e]
      simp only [Flick.inst, Nat.sub_self, List.length_cons, List.length_nil,
        List.getElem_cons_zero]
      apply Flick.Tm.subst_fix
      intro j _
      simp only [Function.comp, Flick.inst]
      rw [dif_neg (by omega)]; congr 1; omega
    · simp only [Flick.inst, List.length_cons, List.length_nil,
        List.length_append]
      rw [dif_neg (by omega : ¬ i - ws.length < 1), dif_neg (by omega : ¬ i < ws.length + 1)]
      simp only [Flick.Tm.subst, Function.comp, Flick.inst]
      rw [dif_neg (by omega)]; congr 1; omega

/-- `λ^n. D` applied to `n` arguments with values `vs` steps to `D` with the values; the
first argument is the innermost variable's outermost binder. -/
theorem beta_chain {args vs : List Flick.Tm}
    (h : All2 (fun a v => Star (PStep root) a v ∧ Value v) args vs) (D : Flick.Tm) :
    Star (PStep root) (appsC (lamsC args.length D) args) (D.subst (Flick.inst vs.reverse)) := by
  induction h generalizing D with
  | nil =>
    simp only [appsC, List.foldl, List.reverse_nil]
    rw [Flick.Tm.subst_fix D _ fun i _ => by simp [Flick.inst]]; exact .refl
  | @cons a v rest vs' hav hr ih =>
    obtain ⟨sa, hv⟩ := hav
    show Star (PStep root) (appsC (.app (.lam (lamsC rest.length D)) a) rest) _
    have s₁ : Star (PStep root) (appsC (.app (.lam (lamsC rest.length D)) a) rest)
        (appsC (.app (.lam (lamsC rest.length D)) v) rest) :=
      (sa.frame (F := .appR (.lam _)) Value.lam).apps rest
    have s₂ : PStep root (appsC (.app (.lam (lamsC rest.length D)) v) rest)
        (appsC (lamsC rest.length (D.subst (liftS rest.length (Flick.inst [v])))) rest) := by
      rw [← lamsC_subst]; exact (PStep.head (E := []) OkE.nil (.beta hv)).apps rest
    refine s₁.trans (.step s₂ ?_)
    have e : (D.subst (liftS rest.length (Flick.inst [v]))).subst (Flick.inst vs'.reverse) =
        D.subst (Flick.inst (v :: vs').reverse) := by
      rw [Flick.Tm.subst_subst, List.reverse_cons]; congr 1; funext i
      rw [hr.length, ← List.length_reverse]; exact inst_snoc vs'.reverse v i
    rw [← e]; exact ih _

end Flick.Elab
