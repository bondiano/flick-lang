import Formal.RT.Pres2
import Formal.RT.Pres3
import Formal.RT.Pres4
import Formal.RT.Pres5
import Formal.RT.Pres7
import Formal.RT.Pres8
import Formal.RT.Pres9

/-! Every reachable configuration satisfies `Inv`. -/

namespace Flick.RT

variable {T V : Type} [DecidableEq V] {K : Core T V} {L : Lib V} {C C' : Conf T V}

theorem Inv.step (h : Hyp L) (I : Inv C) (hs : Step K L C C') : Inv C' :=
  ⟨step_fresh I hs, step_evIds I hs, step_hist I hs, step_root I hs, step_stJ I hs,
   step_empty I hs, step_finSorted h I hs, step_finIn h I hs, step_finAll h I hs,
   step_fsNil I hs, step_regJ h I hs, step_live I hs, step_guard h I hs, step_frames h I hs,
   step_held I hs, step_groups I hs, step_spawnJ I hs, step_startJ I hs, step_doneJ I hs,
   step_watchJ I hs, step_extJ h I hs, step_defectJ I hs, step_closeΛ I hs, step_ents I hs⟩

omit [DecidableEq V] in
theorem Inv.initial (e₀ : T) (unit : V) : Inv (init e₀ unit) := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_,
    ?_, ?_, ?_⟩ <;>
    (try simp [RT.init, upd, Hist, Owner.fresh, St.live, StartedIn, Guard, FrOk, GroupOk]) <;>
    first | omega | (intro π o t _ ho; subst ho; simp)

theorem Inv.reach (h : Hyp L) {e₀ : T} (hr : Reach K L e₀ C) : Inv C := by
  induction hr with
  | refl => exact Inv.initial e₀ K.unit
  | step _ hs ih => exact ih.step h hs

end Flick.RT
