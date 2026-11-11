import Formal.Elab.Witness.Sig

/-!
# Witness: `Real P Sg N`

The signature `Sg` declares what the program needs: the record of each class and number of
parameters with its superclass slot and its methods, and the globals of the instances, the
constants and the structural `Eq` of `Rec`.
-/

namespace Flick.Elab.Witness

open Flick.Elab

theorem cls_div (C n : Nat) : N.cls C n / 8 = n ∧ N.cls C n % 8 = if C < 5 then C else 7 := by
  simp only [N]; split <;> omega

theorem dict_cls {C n fs} (h : Sg.dict (N.cls C n) = some (n, fs)) :
    fs = dfields (if C < 5 then C else 7) n := by
  simp only [Sg, Option.some.injEq, Prod.mk.injEq] at h
  rw [← h.2, (cls_div C n).2, (cls_div C n).1]


theorem real : Real P Sg N where
  dict C n := ⟨_, by simp only [Sg]; rw [(cls_div C n).1]⟩
  slot {C n fs i Sup} h hs := by
    rw [dict_cls h]
    simp only [Prog.super, P] at hs
    unfold supers at hs
    split at hs <;> (rcases i with _ | i <;> simp at hs) <;> subst hs <;> rfl
  meth {m C s} h := by
    refine ⟨1, [], ?_⟩
    simp only [P] at h
    unfold methSch at h
    split at h <;> cases h <;> refine ⟨rfl, by decide, ?_, ?_, ?_⟩ <;>
      first
      | (intro fs hf; rw [dict_cls hf]; rfl)
      | exact TTy.closed_of fun n hn => by
          simp [Ty.Occurs, bindTy, map2Ty, appQ, arrP, boolTy] at hn ⊢; omega
      | (intro σ σ' hσ; simp [Bound.subst, Ty.subst, hσ]; done)
  inst {I ins} h := by
    refine ⟨0, ?_, ?_⟩
    · simp only [Sg, globF, P, N] at h ⊢
      rw [if_pos (by omega), show 4 * I / 4 = I by omega, h]; rfl
    · simp only [P] at h
      unfold insts at h
      split at h <;> cases h <;> intro σ σ' _ <;> rfl
  const {c s} h := by
    simp only [Sg, globF, P, N] at h ⊢
    rw [if_neg (by omega), if_pos (by omega), show (4 * c + 1) / 4 = c by omega, h]; rfl
  strct {C τ ts} hC hτ := by
    simp only [P] at hC hτ
    split at hτ <;> cases hτ; subst_vars
    simp at hC; subst hC
    exact ⟨0, rfl, fun _ _ _ => rfl⟩
  prim h := by cases h
  some := rfl
  some_res := rfl
  ctor h := by cases h
  ctor_holds h := by cases h
  con_inj h := by cases h

end Flick.Elab.Witness
