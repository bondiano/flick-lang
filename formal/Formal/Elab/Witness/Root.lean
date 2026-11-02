import Formal.Elab.Witness.Inst
import Formal.Elab.CoreDictSem

/-!
# Witness: the bodies of the globals, `RootTy` and `RealRun` (W2)

`root` has a body for every global of `Sg`: the instances (`instBody`), the constants and
the structural `Eq` of `Rec`. The bodies of `Seq.flat_map`, `each_φ`, `from_result` and
`unc n` are `todo` (typed at any type): the core has neither list recursion in these
bodies nor a raising `R` with an empty row of errors, and the hypotheses ask only for a typed
body. So are the method `eq` of `Rec` and, as a constant, `eq` on `Int` (the core has no
comparison of `Int`).
-/

namespace Flick.Elab.Witness

open Flick.Elab

def constBody : Nat → Option Flick.Tm
  | 0 => some pairB
  | 1 => some (.ctor "None")
  | 3 => some seqSingleton
  | 4 => some seqEmpty
  | 5 => some seqToList
  | 8 => some (.ctor "MkUnit")
  | 2 | 6 | 7 | 9 => some .todo
  | n + 10 => if 0 < n then some .todo else none

def strctBody : Flick.Tm := .lam (.drec (N.cls 0 1) (.rcons "eq" .todo .rnil))

def bodyF (g : Nat) : Option Flick.Tm :=
  if g % 4 = 0 then instBody (g / 4)
  else if g % 4 = 1 then constBody (g / 4)
  else if g = 2 then some strctBody
  else none

def root : Roots := ⟨fun _ => none, bodyF⟩

theorem const_typ {c s b} (hc : constSch c = some s) (hb : constBody c = some b) :
    (∀ x, ¬ b.Has x) ∧ Typ Sg [] Facts.none [] b (.ex (trTT N (dicts .cnil s.ctx (.mono s.ty))) .pure) := by
  unfold constSch at hc
  split at hc
  rotate_right
  · split at hc
    · cases hc; simp only [constBody, if_pos (by omega : 0 < _), Option.some.injEq] at hb; subst hb
      exact ⟨fun _ h => h, .todo⟩
    · cases hc
  all_goals cases hc; simp only [constBody, Option.some.injEq] at hb; subst hb
  · exact ⟨fun _ h => by simp [pairB, Flick.Tm.Has] at h,
      typ_pairB fun x h => .inr (.inl (.inl (.inr h)))⟩
  · exact ⟨fun _ h => h, typ_con0 (us := [.tvar 0]) rfl⟩
  · exact ⟨fun _ h => h, .todo⟩
  · exact ⟨fun _ h => by simp [seqSingleton, Flick.Tm.Has] at h, typ_seqSingleton⟩
  · exact ⟨fun _ h => by simp [seqEmpty, Flick.Tm.Has] at h, typ_seqEmpty⟩
  · exact ⟨fun _ h => by simp [seqToList, un1, Flick.Tm.Has] at h, typ_seqToList⟩
  · exact ⟨fun _ h => h, .todo⟩
  · exact ⟨fun _ h => h, .todo⟩
  · exact ⟨fun _ h => h, typ_con0 (us := []) rfl⟩
  · exact ⟨fun _ h => h, .todo⟩

theorem strct_typ : (∀ x, ¬ strctBody.Has x) ∧
    Typ Sg [] Facts.none [] strctBody (.ex (trTT N (strctTy cEq tyRec [tyInt])) .pure) :=
  ⟨fun _ h => by simp [strctBody, Flick.Tm.Has] at h,
   typ_lam (.drec (us := [trT N (.con 13)]) rfl rfl (.dcons .todo .dnil) (fun h => by
      simp [Flick.Tm.Has] at h) (held_sub (fun _ h => by simp [Flick.Tm.Has] at h)
      (mem1 fun x h => absurd ((Flick.Scheme.reach_mono _ _).1 h) (trD_reach N _ x))))
    (fun _ h => by simp [Flick.Tm.Has] at h) (by simp)⟩

theorem inst_body {I ins} (h : insts I = some ins) : ∃ b, instBody I = some b ∧ ∀ x, ¬ b.Has x := by
  unfold insts at h
  split at h <;> cases h <;> refine ⟨_, rfl, fun x h => ?_⟩ <;>
    simp [eqInt, effPure, effMap2, effBind, mkEff, unEff, optPure, optMap2, optBind, un1, someQ,
      noneQ, iterL, Flick.Tm.Has] at h

theorem instBody_some {I b} (h : instBody I = some b) : ∃ ins, insts I = some ins := by
  unfold instBody at h; split at h <;> cases h <;> exact ⟨_, rfl⟩

theorem constBody_some {c b} (h : constBody c = some b) : ∃ s, constSch c = some s := by
  unfold constBody at h; split at h <;> (try cases h) <;> (try exact ⟨_, rfl⟩)
  rename_i n; split at h
  · exact ⟨uncSch 10 n, by simp [constSch, *]⟩
  · cases h

theorem constSch_body {c s} (h : constSch c = some s) : ∃ b, constBody c = some b := by
  unfold constSch at h; split at h <;> (try cases h) <;> (try exact ⟨_, rfl⟩)
  rename_i n; split at h
  · exact ⟨Flick.Tm.todo, by simp [constBody, *]⟩
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
