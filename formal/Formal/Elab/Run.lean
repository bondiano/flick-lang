import Formal.Elab.Target
import Formal.Elab.Coherence

/-!
# Elaboration: the simulation `Sim` from the steps of the target (elaboration.md §4.2)

A skeleton is a `Tm` with dictionary holes `hole i`, with the type and stage of its fillings;
filling the holes with `ds` gives a term, and running it gives the results of the terminating
runs of its loaded state. The simulation is about fillings that are closed typed terms. `Target.sim`
derives `Sim` from the local simulation of `Target` (`load_rel`, `step_rel`, `final_rel`) by
induction on the run; `T6b_target` is T6(b) without the hypothesis `Sim`.
-/

namespace Flick.Elab

variable {P : Prog}

/-- Fill the holes of a skeleton with `ds`. -/
def Tm.fill (ds : List Dict) : Tm → Tm
  | .var x => .var x
  | .hole i => .dict (args ds i)
  | .dict d => .dict d
  | .meth e m => .meth (e.fill ds) m
  | .app f a => .app (f.fill ds) (a.fill ds)
  | .lam b => .lam (b.fill ds)
  | .let_ e b => .let_ (e.fill ds) (b.fill ds)
  | .lamD l b => .lamD l (b.fill ds)
  | .const c => .const c
  | .ite c a b => .ite (c.fill ds) (a.fill ds) (b.fill ds)
  | .caseOpt e n s => .caseOpt (e.fill ds) (n.fill ds) (s.fill ds)

theorem args_eqv {ds ds' : List Dict} (h : All2 (Eqv P) ds ds') (i : Nat) :
    Eqv P (args ds i) (args ds' i) := by
  induction h generalizing i with
  | nil => exact .refl
  | cons r _ ih => cases i with
    | zero => exact r
    | succ i => exact ih i

theorem fill_eqv {ds ds' : List Dict} (h : All2 (Eqv P) ds ds') (s : Tm) :
    TmEqv P (s.fill ds) (s.fill ds') := by
  induction s with
  | var => exact .var
  | hole i => exact .dict (args_eqv h i)
  | dict => exact .dict .refl
  | meth _ _ ih => exact .meth ih
  | app _ _ ihf iha => exact .app ihf iha
  | lam _ ih => exact .lam ih
  | let_ _ _ ihe ihb => exact .let_ ihe ihb
  | lamD _ _ ih => exact .lamD ih
  | const => exact .const
  | ite _ _ _ ihc iha ihb => exact .ite ihc iha ihb
  | caseOpt _ _ _ ihe ihn ihs => exact .caseOpt ihe ihn ihs

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
  Ok s ds := X.Has [] [] (s.1.fill ds) s.2.1 s.2.2
  run s ds o := ∃ v, Star X.Step (X.load (s.1.fill ds)) v ∧ X.obs v = some o

theorem Target.run_mono {s : X.exprs.Skel} {ds ds' : List Dict} (h : All2 (Eqv P) ds ds')
    (o₁ : X.exprs.Ok s ds) (o₂ : X.exprs.Ok s ds') {o : X.Obs} (hr : X.exprs.run s ds o) :
    X.exprs.run s ds' o := by
  obtain ⟨v, hv, ho⟩ := hr
  exact X.run_rel (X.load_rel (fill_eqv h s.1) o₁ o₂) hv ho

/-- **The simulation of T6(b)**, from the steps of the target. -/
theorem Target.sim : Sim P X.exprs := fun _ _ _ h o₁ o₂ =>
  funext fun _ => propext ⟨X.run_mono h o₁ o₂, X.run_mono h.symm o₂ o₁⟩

/-- **T6(b)** for the skeletons of a target: the hypothesis `Sim` is `Target.sim`. -/
theorem T6b_target {lt : Cn → Cn → Prop} (h : Coherent P lt) (s : Tm) {t : TTy} {φ : Stage}
    {Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict} (hρ : Closes P Δ θ ρ) {qs : List Cn}
    {ds₁ ds₂ : List Dict} (h₁ : Ent P Δ qs ds₁) (h₂ : Ent P Δ qs ds₂)
    (o₁ : X.Has [] [] (s.fill (ds₁.map (·.subst ρ))) t φ)
    (o₂ : X.Has [] [] (s.fill (ds₂.map (·.subst ρ))) t φ) :
    All2 (Eqv P) (ds₁.map (·.subst ρ)) (ds₂.map (·.subst ρ)) ∧
      X.exprs.run (s, t, φ) (ds₁.map (·.subst ρ)) = X.exprs.run (s, t, φ) (ds₂.map (·.subst ρ)) :=
  T6b h (X.sim) (s, t, φ) hρ h₁ h₂ o₁ o₂

end Flick.Elab
