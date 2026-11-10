import Formal.Elab.Witness.Unc

/-!
# Witness: the bodies of the instances typed at their globals

An instance is the record `drec (N.cls C 1) …` of its slot (the global of the superclass's
instance) and its methods, each typed at its field by `field_inst`.
-/

namespace Flick.Elab.Witness

open Flick.Elab

/-- `eq` on `Int` is the constant of the core. -/
def eqInt : Flick.Tm := .const .intEq

def instBody : Nat → Option Flick.Tm
  | 0 => some (.drec (N.cls 0 1) (.rcons "eq" eqInt .rnil))
  | 1 => some (.drec (N.cls 1 1) (.rcons "pure" (effPure "MkEff") (.rcons "map2" (effMap2 "MkEff") .rnil)))
  | 2 => some (.drec (N.cls 2 1) (.rcons "slot" (.glob 4) (.rcons "flat_map" (effBind "MkEff") .rnil)))
  | 3 => some (.drec (N.cls 1 1) (.rcons "pure" optPure (.rcons "map2" optMap2 .rnil)))
  | 4 => some (.drec (N.cls 2 1) (.rcons "slot" (.glob 12) (.rcons "flat_map" optBind .rnil)))
  | 5 => some (.drec (N.cls 3 1) (.rcons "slot" (.glob 12) (.rcons "nothing" noneQ .rnil)))
  | 6 => some (.drec (N.cls 4 1) (.rcons "iter" iterL .rnil))
  | _ => none

theorem mfield_t (s : Scheme) (c : Nat) :
    ((mfield s).substT (instT [trT N (.con c)])).t = trT N (s.ty.subst (inst [.con c])) := by
  simp only [Flick.Scheme.substT, mfield]; exact field_inst N s.ty c _

theorem hmk : Sg.ctor "MkEff" = some (2, [effField], "Eff") := rfl

theorem hmkr : Sg.res "MkEff" = idTs 2 := rfl

theorem hone_eff : ∀ K k ts, Sg.ctor K = some (k, ts, "Eff") → K = "MkEff" := by
  intro K k ts h; simp only [Sg] at h; unfold ctorF at h; split at h <;> simp_all

variable {Ψ : List Flick.Ty} {Θ : Facts}

theorem dcons_m {s : Scheme} {c : Nat} {v r : Flick.Tm} {fs l}
    (hv : ∀ Ψ Θ, TypM Sg .live Ψ Θ [] v (.ex (trT N (s.ty.subst (inst [.con c]))) .pure))
    (hr : TypM Sg .live Ψ Θ [] r (.dfs fs)) :
    TypM Sg .live Ψ Θ [] (.rcons l v r) (.dfs ((l, (mfield s).substT (instT [trT N (.con c)])) :: fs)) :=
  .dcons (by rw [mfield_t]; exact hv _ _) hr

theorem typ_optBind_tr (c d a b e : Ty) {φ} :
    TypM Sg .live Ψ Θ [] optBind (.ex (trT N (bindTy (.con 6) c d a b e)) φ) := by
  simp only [bindTy, arrP, appQ, trT, if_neg (show (6 : Nat) ≠ 2 by decide),
    if_neg (show (6 : Nat) ≠ 4 by decide)]
  exact typ_optBind

theorem typ_optMap2_tr (c d a b x g d' : Ty) {φ} :
    TypM Sg .live Ψ Θ [] optMap2 (.ex (trT N (map2Ty (.con 6) c d a b x g d')) φ) := by
  simp only [map2Ty, arrP, appQ, trT, if_neg (show (6 : Nat) ≠ 2 by decide),
    if_neg (show (6 : Nat) ≠ 4 by decide)]
  exact typ_optMap2

theorem inst_typ {I ins b} (hI : insts I = some ins) (hb : instBody I = some b) :
    TypM Sg .live [] Facts.none [] b (.ex (trTT N (instTy ins)) .pure) := by
  unfold insts at hI
  split at hI <;> cases hI <;> simp only [instBody, Option.some.injEq] at hb <;> subst hb <;>
    refine .drec (us := [trT N (.con _)]) rfl rfl ?_ (fun h => by
      simp [eqInt, effPure, effMap2, effBind, mkEff, unEff, optPure, optMap2, optBind, un1, someQ,
        noneQ, iterL, caseLQ, scons, snil, pvars, Flick.Tm.Has] at h) trivial
  · exact dcons_m (s := ((methSch 0).get rfl).2) (fun _ _ => .const (ι := default) trivial .refl) .dnil
  · exact dcons_m (s := ((methSch 1).get rfl).2) (fun _ _ => typ_effPure_tr hmk hmkr N rfl (by decide) (by decide) _ _)
      (dcons_m (s := ((methSch 2).get rfl).2)
        (fun _ _ => typ_effMap2_tr hmk hmkr hone_eff N rfl (by decide) (by decide) _ _ _ _ _ _ _) .dnil)
  · exact .dcons (.glob (ts := []) rfl rfl (fun _ _ h => h.elim) .refl) (dcons_m (s := ((methSch 3).get rfl).2)
      (fun _ _ => typ_effBind_tr hmk hmkr hone_eff N rfl (by decide) (by decide) _ _ _ _ _) .dnil)
  · exact dcons_m (s := ((methSch 1).get rfl).2)
      (fun _ _ => typ_optPure)
      (dcons_m (s := ((methSch 2).get rfl).2) (fun _ _ => typ_optMap2_tr _ _ _ _ _ _ _) .dnil)
  · exact .dcons (.glob (ts := []) rfl rfl (fun _ _ h => h.elim) .refl) (dcons_m (s := ((methSch 3).get rfl).2)
      (fun _ _ => typ_optBind_tr _ _ _ _ _) .dnil)
  · exact .dcons (.glob (ts := []) rfl rfl (fun _ _ h => h.elim) .refl) (dcons_m (s := ((methSch 4).get rfl).2)
      (fun _ _ => typ_noneQ) .dnil)
  · exact dcons_m (s := ((methSch 5).get rfl).2) (fun _ _ => typ_iterL) .dnil

end Flick.Elab.Witness
