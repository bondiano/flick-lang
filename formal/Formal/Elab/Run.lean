import Formal.Elab.Target
import Formal.Elab.Coherence

/-!
# Elaboration: the simulation `Sim` from the steps of the target (elaboration.md §4.2)

A skeleton is a `Tm` with dictionary holes `hole i`, with the type and stage of its fillings;
filling the holes with `ds` gives a term, and running it gives the results of the terminating
runs of its loaded state. The simulation is about fillings that are closed typed terms, with
typed dictionaries in the holes. `Target.sim` derives `Sim` from the local simulation of
`Target` (`load_rel`, `step_rel`, `final_rel`) by induction on the run; `T6b_target`
(`Close.lean`) is T6(b) without the hypothesis `Sim`.
-/

namespace Flick.Elab

variable {P : Prog}

theorem args_eqv {ds ds' : List Dict} (h : All2 (Eqv P) ds ds') (i : Nat) :
    Eqv P (args ds i) (args ds' i) := by
  induction h generalizing i with
  | nil => exact .refl
  | cons r _ ih => cases i with
    | zero => exact r
    | succ i => exact ih i

theorem All2.symm {ds ds' : List Dict} (h : All2 (Eqv P) ds ds') : All2 (Eqv P) ds' ds := by
  induction h with
  | nil => exact .nil
  | cons r _ ih => exact .cons (.symm r) ih

theorem Star.trans {α : Type} {R : α → α → Prop} {a b c : α} (h₁ : Star R a b) (h₂ : Star R b c) :
    Star R a c := by
  induction h₁ with
  | refl => exact h₂
  | step r _ ih => exact .step r (ih h₂)

variable (X : Target P)

/-- A run of `a` to a result `o` is matched from any state related to `a`. -/
theorem Target.run_rel {a v b : X.St} {o : X.Obs} (hr : X.Rel a b) (h : Star X.Step a v)
    (ho : X.obs v = some o) : ∃ w, Star X.Step b w ∧ X.obs w = some o := by
  induction h generalizing b with
  | refl => exact X.final_rel hr ho
  | step s _ ih =>
    obtain ⟨b₁, hb, hr₁⟩ := X.step_rel hr s
    obtain ⟨w, hw, hwo⟩ := ih hr₁ ho
    exact ⟨w, hb.trans hw, hwo⟩

/-- Skeletons of the target, typed at `t ! φ`, and the results of their runs. -/
def Target.exprs : Exprs where
  Skel := Tm × TTy × Stage
  Out := X.Obs → Prop
  Ok s ds := X.Has [] [] (s.1.fill ds) s.2.1 s.2.2 ∧
    ∀ (i : Nat) (d : Dict), ds[i]? = some d → ∃ q, X.Has [] [] (.dict d) (.dict q) .P
  run s ds o := ∃ v, Star X.Step (X.load (s.1.fill ds)) v ∧ X.obs v = some o

theorem Target.run_mono {s : X.exprs.Skel} {ds ds' : List Dict} (h : All2 (Eqv P) ds ds')
    (o₁ : X.exprs.Ok s ds) (o₂ : X.exprs.Ok s ds') {o : X.Obs} (hr : X.exprs.run s ds o) :
    X.exprs.run s ds' o := by
  obtain ⟨v, hv, ho⟩ := hr
  exact X.run_rel (X.load_rel h o₁.2 o₁.1 o₂.1) hv ho

/-- **The simulation of T6(b)**, from the steps of the target. -/
theorem Target.sim : Sim P X.exprs := fun _ _ _ h o₁ o₂ =>
  funext fun _ => propext ⟨X.run_mono h o₁ o₂, X.run_mono h.symm o₂ o₁⟩

end Flick.Elab
