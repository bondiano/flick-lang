import Formal.Elab.Witness.Inst
import Formal.Elab.CoreDictSem

/-!
# Witness: the bodies of the globals, `RootTy` and `RealRun`

`root` has a body for every global of `Sg`: the instances (`instBody`), the constants and
the structural `Eq` of `Rec`, each typed by the rules of the core at its scheme. No body is
`todo`: the lists of `Seq.lean`, `unc n`, `from_result` and the structural `eq` of `Unc.lean`,
and `eq` on `Int`, the constant `intEq` (`Inst.lean`).
-/

namespace Flick.Elab.Witness

open Flick.Elab

def constBody : Nat → Option Flick.Tm
  | 0 => some pairB
  | 1 => some (.ctor "None")
  | 2 => some seqFlatMap
  | 3 => some seqSingleton
  | 4 => some seqEmpty
  | 5 => some seqToList
  | 6 | 7 => some seqEach
  | 8 => some (.ctor "MkUnit")
  | 9 => none
  | n + 10 => if n % 2 = 0 then (if 0 < n / 2 then some (uncB (n / 2 - 1)) else none)
      else if n = 1 then some frBody else none

def strctBody : Flick.Tm := .lam (.drec (N.cls 0 1) (.rcons "eq" eqRec .rnil))

def bodyF (g : Nat) : Option Flick.Tm :=
  if g % 4 = 0 then instBody (g / 4)
  else if g % 4 = 1 then constBody (g / 4)
  else if g = 2 then some strctBody
  else none

def root : Roots := ⟨fun _ => none, bodyF⟩

variable {Ψ : List Flick.Ty} {Θ : Facts}

theorem typ_seqFlatMap_tr (c d a b e : Ty) {φ} :
    Typ Sg Ψ Θ [] seqFlatMap (.ex (trT N (bindTy (.con 9) c d a b e)) φ) := by
  simp only [bindTy, arrP, appQ, trT, if_neg (show (9 : Nat) ≠ 2 by decide),
    if_neg (show (9 : Nat) ≠ 4 by decide)]
  refine typ_seqFlatMap ?_
  intro x h; rcases x with n | p <;>
    simp [seqT, N, Flick.Ty.reach, trT_reach, Caps.none] at h ⊢ <;> grind

theorem frBody_has (s : Site) : ¬ frBody.Has s := by simp [frBody, Flick.Tm.Has]

theorem const_typ {c s b} (hc : constSch c = some s) (hb : constBody c = some b) :
    (∀ x, ¬ b.Has x) ∧ Typ Sg [] Facts.none [] b (.ex (trTT N (dicts .cnil s.ctx (.mono s.ty))) .pure) := by
  unfold constSch at hc
  split at hc
  rotate_right
  · rename_i n
    split at hc
    · rename_i h2
      split at hc
      · rename_i h0
        cases hc
        simp only [constBody, if_pos h2, if_pos h0, Option.some.injEq] at hb; subst hb
        obtain ⟨m, hm⟩ : ∃ m, n / 2 = m + 1 := ⟨n / 2 - 1, by omega⟩
        rw [hm, Nat.add_sub_cancel]
        exact ⟨uncB_has m, typ_unc_tr m⟩
      · cases hc
    · rename_i h2
      split at hc
      · rename_i h1
        cases hc
        simp only [constBody, if_neg h2, if_pos h1, Option.some.injEq] at hb; subst hb
        exact ⟨frBody_has, typ_frBody fun _ h => .inr h⟩
      · cases hc
  all_goals cases hc <;> simp only [constBody, Option.some.injEq] at hb <;> subst hb
  · exact ⟨fun _ h => by simp [pairB, Flick.Tm.Has] at h,
      typ_pairB fun x h => .inr (.inl (.inl (.inr h)))⟩
  · exact ⟨fun _ h => h, typ_con0 (us := [.tvar 0]) rfl⟩
  · exact ⟨fun _ h => by simp [seqFlatMap, seqApp, caseSeq, scons, snil, Flick.Tm.Has] at h,
      typ_seqFlatMap_tr _ _ _ _ _⟩
  · exact ⟨fun _ h => by simp [seqSingleton, scons, snil, Flick.Tm.Has] at h, typ_seqSingleton⟩
  · exact ⟨fun _ h => by simp [seqEmpty, snil, Flick.Tm.Has] at h, typ_seqEmpty⟩
  · exact ⟨fun _ h => by simp [seqToList, caseSeq, Flick.Tm.Has] at h, typ_seqToList⟩
  · exact ⟨fun _ h => by simp [seqEach, caseSeq, Flick.Tm.Has] at h, typ_seqEach⟩
  · exact ⟨fun _ h => by simp [seqEach, caseSeq, Flick.Tm.Has] at h, typ_seqEach⟩
  · exact ⟨fun _ h => h, typ_con0 (us := []) rfl⟩

theorem typ_eqRec {Ψ Θ} : Typ Sg Ψ Θ [.mono (trD N ⟨cEq, [tyInt]⟩)] eqRec
    (.ex (trT N ((arrP (.var 0) (arrP (.var 0) boolTy)).subst (inst [.con 13]))) .pure) := by
  have hR : ∀ {Γ' : List Flick.Scheme} {e b t},
      Typ Sg Ψ Θ Γ' e (.ex (tc "Rec" []) .pure) →
      Typ Sg Ψ Θ (.mono (.base .int) :: Γ') b (.ex t .pure) →
      Typ Sg Ψ Θ Γ' (.match_ e (.acons (pvars "MkRec" 1) b .anil)) (.ex t .pure) := fun he hb =>
    .match_ he (.acons (.adt (.app (.head (K := "MkRec") (ts := [.base .int]) (us := []) rfl Sig.inj_plain rfl) .var)
      (tcon_hd _ _)) hb .anil) (.inl (exh_one (K := "MkRec") (ts := [.base .int]) rfl hone_rec []))
  refine typ_lam (typ_lam (hR (typ_var_mono rfl) (hR (typ_var_mono rfl)
      (.app (ψ := .pure) (.app (ψ := .pure)
        (.dproj (us := [trT N (.con 4)]) (ts := []) (s := mfield ((methSch 0).get rfl).2)
          (T := trT N ((arrP (.var 0) (arrP (.var 0) boolTy)).subst (inst [.con 4])))
          (typ_var_mono rfl) rfl rfl rfl rfl (fun _ _ ⟨_, _, h, _⟩ => h.elim) ?_)
        (typ_var_mono rfl) trivial .refl) (typ_var_mono rfl) trivial .refl)))
      (fun s h => by simp [Flick.Tm.Has] at h) ?_) (fun s h => by simp [Flick.Tm.Has] at h) ?_
  · rw [Flick.Scheme.inst, substT_instT_nil, mfield_t]; exact .refl
  all_goals
    intro s hs x h
    simp only [List.mem_cons, List.not_mem_nil, or_false] at hs
    rcases hs with rfl | rfl <;>
      first
      | exact .inl ((Flick.Scheme.reach_mono _ _).1 h)
      | simp [trD, Flick.Ty.reach, Caps.none] at h

theorem strct_typ : (∀ x, ¬ strctBody.Has x) ∧
    Typ Sg [] Facts.none [] strctBody (.ex (trTT N (strctTy cEq tyRec [tyInt])) .pure) :=
  ⟨fun _ h => by simp [strctBody, eqRec, pvars, Flick.Tm.Has] at h,
   typ_lam (.drec (us := [trT N (.con 13)]) rfl rfl (.dcons (by
      show Typ Sg ([].map (Flick.Ty.up 0)) _ (List.map (Flick.Scheme.up 0) _) _ _
      rw [List.map_sup_zero, mfield_t]; exact typ_eqRec) .dnil)
      (fun h => by simp [eqRec, pvars, Flick.Tm.Has] at h) (held_sub (fun _ h => by
        simp [eqRec, pvars, Flick.Tm.Has] at h)
      (mem1 fun x h => absurd ((Flick.Scheme.reach_mono _ _).1 h) (trD_reach N _ x))))
    (fun _ h => by simp [eqRec, pvars, Flick.Tm.Has] at h) (by simp)⟩

theorem inst_body {I ins} (h : insts I = some ins) : ∃ b, instBody I = some b ∧ ∀ x, ¬ b.Has x := by
  unfold insts at h
  split at h <;> cases h <;> refine ⟨_, rfl, fun x h => ?_⟩ <;>
    simp [eqInt, effPure, effMap2, effBind, mkEff, unEff, optPure, optMap2, optBind, un1, someQ,
      noneQ, iterL, caseLQ, scons, snil, pvars, Flick.Tm.Has] at h

theorem instBody_some {I b} (h : instBody I = some b) : ∃ ins, insts I = some ins := by
  unfold instBody at h; split at h <;> cases h <;> exact ⟨_, rfl⟩

theorem constBody_some {c b} (h : constBody c = some b) : ∃ s, constSch c = some s := by
  unfold constBody at h; split at h <;> (try cases h) <;> (try exact ⟨_, rfl⟩)
  rename_i n; split at h
  · split at h
    · exact ⟨uncSch 10 (n / 2), by simp [constSch, *]⟩
    · cases h
  · split at h
    · exact ⟨PreludeSchemes.from_result 11 12, by simp [constSch, *]⟩
    · cases h

theorem constSch_body {c s} (h : constSch c = some s) : ∃ b, constBody c = some b := by
  unfold constSch at h; split at h <;> (try cases h) <;> (try exact ⟨_, rfl⟩)
  rename_i n; split at h
  · split at h
    · exact ⟨uncB (n / 2 - 1), by simp [constBody, *]⟩
    · cases h
  · split at h
    · exact ⟨frBody, by simp [constBody, *]⟩
    · cases h

theorem rootTy : RootTy Sg root where
  svc _ _ h := by cases h
  glob g v h := by
    simp only [root, bodyF] at h
    simp only [Sg, globF]
    split at h
    · obtain ⟨ins, hi⟩ := instBody_some h
      obtain ⟨b, hb, hn⟩ := inst_body hi
      rw [hb] at h; cases h
      rw [if_pos (by assumption), hi]
      exact ⟨hn, _, rfl, inst_typ hi hb⟩
    · split at h
      · obtain ⟨s, hs⟩ := constBody_some h
        obtain ⟨hn, ht⟩ := const_typ hs h
        rw [if_neg (by assumption), if_pos (by assumption), hs]
        exact ⟨hn, _, rfl, ht⟩
      · split at h
        · cases h
          rw [if_neg (by assumption), if_neg (by assumption), if_pos (by assumption)]
          exact ⟨strct_typ.1, _, rfl, strct_typ.2⟩
        · cases h
  globs g s h := by
    simp only [Sg, globF] at h
    simp only [root, bodyF]
    split at h
    · rw [if_pos (by assumption)]
      cases e : insts (g / 4) with
      | none => simp [e] at h
      | some ins => obtain ⟨b, hb, -⟩ := inst_body e; simp [hb]
    · rw [if_neg (by assumption)]
      split at h
      · rw [if_pos (by assumption)]
        cases e : constSch (g / 4) with
        | none => simp [e] at h
        | some s => obtain ⟨b, hb⟩ := constSch_body e; simp [hb]
      · rw [if_neg (by assumption)]
        split at h
        · rw [if_pos (by assumption)]; rfl
        · cases h

theorem realRun : RealRun P N root where
  inst {I ins} h := by
    simp only [P] at h
    unfold insts at h
    split at h <;> cases h <;> refine ⟨_, rfl, ?_⟩ <;> intro i s hs <;>
      (rcases i with _ | i <;> simp at hs) <;> subst hs <;> rfl
  strct {C τ ts} hC hτ := by
    simp only [P] at hC hτ
    split at hτ <;> cases hτ; subst_vars
    simp at hC; subst hC
    exact ⟨_, rfl, fun i S hS => by simp [P, supers, cEq] at hS⟩
  strctSub _ _ := rfl
  prim h := by cases h
  primSub h := by cases h

end Flick.Elab.Witness
