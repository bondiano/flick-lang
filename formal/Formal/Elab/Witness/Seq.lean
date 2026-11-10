import Formal.Elab.Witness.Bodies
import Formal.Elab.Exh

/-!
# Witness: the bodies of `Seq` and of `iter`

`Seq q a` and `ListQ q a` are lists of the core, `SNil | SCons a (Seq q a)`, whose `CRow`
parameter `q` is phantom: the model does not need `Seq` to be lazy, and its schemes are those of
the `CRow` qualifiers of the Prelude schemes. `Seq.flat_map`, `each_φ`, `to_list` and `iter` are fixpoints that match the list by one
arm per constructor (`exh_ctors`). `flat_map k` appends `k x` to the rest by a second fixpoint.
-/

namespace Flick.Elab.Witness

open Flick.Elab

variable {Ψ : List Flick.Ty} {Θ : Facts} {Γ : List Flick.Scheme}

def snil : Flick.Tm := .ctor "SNil"
def scons (x xs : Flick.Tm) : Flick.Tm := .app (.app (.ctor "SCons") x) xs

/-- `match e with SCons x xs → c | SNil → n` (`x` is `var 0`, `xs` is `var 1` in `c`). -/
def caseSeq (e c n : Flick.Tm) : Flick.Tm :=
  .match_ e (.acons (pvars "SCons" 2) c (.acons (pvars "SNil" 0) n .anil))

/-- `match e with ConsQ x xs → c | NilQ → n`. -/
def caseLQ (e c n : Flick.Tm) : Flick.Tm :=
  .match_ e (.acons (pvars "ConsQ" 2) c (.acons (pvars "NilQ" 0) n .anil))

theorem typ_list2 {K₁ K₀ T f e c n D A t φ}
    (h₁ : Sg.ctor K₁ = some (2, [.tvar 1, f], T)) (h₀ : Sg.ctor K₀ = some (2, [], T))
    (hall : ∀ K k ts, Sg.ctor K = some (k, ts, T) → pvars K ts.length ∈ [pvars K₁ 2, pvars K₀ 0])
    (he : Typ Sg Ψ Θ Γ e (.ex (tc T [D, A]) φ))
    (hc : Typ Sg Ψ Θ (.mono A :: .mono (f.substT (instT [D, A])) :: Γ) c (.ex t φ))
    (hn : Typ Sg Ψ Θ Γ n (.ex t φ)) :
    Typ Sg Ψ Θ Γ (.match_ e (.acons (pvars K₁ 2) c (.acons (pvars K₀ 0) n .anil))) (.ex t φ) :=
  .match_ he (.acons (.adt (.app (.app (PatTy.head_plain Sg (us := [D, A]) h₁ (sg_plain h₁) rfl)
      .var) .var) (tcon_hd T _)) hc
    (.acons (.adt (PatTy.head_plain Sg (us := [D, A]) h₀ (sg_plain h₀) rfl) (tcon_hd T _)) hn .anil))
    (.inl (exh_ctors hall _))

theorem hall_seq : ∀ K k ts, Sg.ctor K = some (k, ts, "Seq") →
    pvars K ts.length ∈ [pvars "SCons" 2, pvars "SNil" 0] := by
  intro K k ts h; simp only [Sg] at h; unfold ctorF at h
  split at h <;> simp at h <;> (obtain ⟨rfl, rfl⟩ := h; simp)

theorem hall_lq : ∀ K k ts, Sg.ctor K = some (k, ts, "ListQ") →
    pvars K ts.length ∈ [pvars "ConsQ" 2, pvars "NilQ" 0] := by
  intro K k ts h; simp only [Sg] at h; unfold ctorF at h
  split at h <;> simp at h <;> (obtain ⟨rfl, rfl⟩ := h; simp)

theorem typ_caseSeq {e c n D A t φ} (he : Typ Sg Ψ Θ Γ e (.ex (seqT D A) φ))
    (hc : Typ Sg Ψ Θ (.mono A :: .mono (seqT D A) :: Γ) c (.ex t φ))
    (hn : Typ Sg Ψ Θ Γ n (.ex t φ)) : Typ Sg Ψ Θ Γ (caseSeq e c n) (.ex t φ) :=
  typ_list2 rfl rfl hall_seq he hc hn

theorem typ_caseLQ {e c n D A t φ} (he : Typ Sg Ψ Θ Γ e (.ex (lqT D A) φ))
    (hc : Typ Sg Ψ Θ (.mono A :: .mono (lqT D A) :: Γ) c (.ex t φ))
    (hn : Typ Sg Ψ Θ Γ n (.ex t φ)) : Typ Sg Ψ Θ Γ (caseLQ e c n) (.ex t φ) :=
  typ_list2 rfl rfl hall_lq he hc hn

theorem typ_scons {x xs Q A φ} (hx : Typ Sg Ψ Θ Γ x (.ex A φ)) (hxs : Typ Sg Ψ Θ Γ xs (.ex (seqT Q A) φ)) :
    Typ Sg Ψ Θ Γ (scons x xs) (.ex (seqT Q A) φ) := typ_con2 (us := [Q, A]) rfl hx hxs

theorem typ_snil {Q A φ} : Typ Sg Ψ Θ Γ snil (.ex (seqT Q A) φ) := typ_con0 (us := [Q, A]) rfl

/-- `fix f`: its body runs at the stage of its arrow with the qualifier live, where it calls
itself. -/
theorem typ_fix {a ψ r f φ} {C : Caps}
    (hf : Typ Sg Ψ Θ (.mono a :: .mono (.arrow a ψ r C) :: Γ) f
      (.ex r (Stage.use ψ C))) (hn : ∀ s, ¬ f.Has s) :
    Typ Sg Ψ Θ Γ (.fix f) (.ex (.arrow a ψ r C) φ) :=
  .fix hf (hn .ins) .refl

/-- `iter xs`: `ConsQ x xs ↦ SCons x (iter xs)`, `NilQ ↦ SNil`. -/
def iterL : Flick.Tm := .fix (caseLQ (.var 0) (scons (.var 0) (.app (.var 3) (.var 1))) snil)
def seqSingleton : Flick.Tm := .lam (scons (.var 0) snil)
def seqEmpty : Flick.Tm := snil
/-- `to_list s`: `SCons x xs ↦ Cons x (to_list xs)`, `SNil ↦ Nil`. -/
def seqToList : Flick.Tm := .fix (caseSeq (.var 0)
  (.app (.app (.ctor "Cons") (.var 0)) (.app (.var 3) (.var 1))) (.ctor "Nil"))
/-- `each_φ f s`: `SCons x xs ↦ let _ = f x in each_φ f xs`, `SNil ↦ ()`. -/
def seqEach : Flick.Tm := .lam (.fix (caseSeq (.var 0)
  (.let_ (.app (.var 4) (.var 0)) (.app (.var 4) (.var 2))) (.ctor "MkUnit")))
/-- `ys ++ t` with `t` at `var 2` of the arm `SNil`: a fixpoint over `ys`. -/
def seqApp : Flick.Tm := caseSeq (.var 0) (scons (.var 0) (.app (.var 3) (.var 1))) (.var 2)
/-- `flat_map k s`: `SCons x xs ↦ let t = flat_map k xs in k x ++ t`, `SNil ↦ SNil`. -/
def seqFlatMap : Flick.Tm := .lam (.fix (caseSeq (.var 0)
  (.let_ (.app (.var 3) (.var 1)) (.app (.fix seqApp) (.app (.var 5) (.var 1)))) snil))

theorem typ_iterL {D Q A C0 φ} :
    Typ Sg Ψ Θ [] iterL (.ex (.arrow (lqT D A) .pure (seqT Q A) C0) φ) :=
  typ_fix (typ_caseLQ (typ_var_mono rfl)
      (typ_scons (typ_var_mono rfl) (.app (ψ := .pure) (typ_var_mono rfl) (typ_var_mono rfl) trivial .refl))
      typ_snil)
    (fun s h => by simp [caseLQ, scons, snil, Flick.Tm.Has] at h)

theorem typ_seqSingleton {Q A C0 φ} :
    Typ Sg Ψ Θ [] seqSingleton (.ex (.arrow A .pure (seqT Q A) C0) φ) :=
  typ_lam (typ_scons (typ_var_mono rfl) typ_snil) (fun s h => by simp [scons, snil, Flick.Tm.Has] at h)

theorem typ_seqEmpty {Q A φ} : Typ Sg Ψ Θ [] seqEmpty (.ex (seqT Q A) φ) := typ_snil

theorem typ_seqToList {D A C0 φ} :
    Typ Sg Ψ Θ [] seqToList (.ex (.arrow (seqT D A) .pure (listT A) C0) φ) :=
  typ_fix (typ_caseSeq (typ_var_mono rfl)
      (typ_con2 (us := [A]) rfl (typ_var_mono rfl) (.app (ψ := .pure) (typ_var_mono rfl) (typ_var_mono rfl) trivial .refl))
      (typ_con0 (us := [A]) rfl))
    (fun s h => by simp [caseSeq, Flick.Tm.Has] at h)

/-- The loop at the later stage `ψ'` with the qualifier `C'`, where what `f x` needs live, its
stage `ψ` with `C`, holds. -/
theorem typ_seqEach_at {A D C C' C0 ψ ψ' φ} (h : (Stage.use ψ C).le (Stage.use ψ' C')) :
    Typ Sg Ψ Θ [] seqEach (.ex (.arrow (.arrow A ψ (tc "Unit" []) C) .pure
      (.arrow (seqT D A) ψ' (tc "Unit" []) C') C0) φ) := by
  exact typ_lam (typ_fix (typ_caseSeq (typ_var_mono rfl)
      (.let_ (.app (typ_var_mono rfl) (typ_var_mono rfl) h .refl)
        (.app (typ_var_mono rfl) (typ_var_mono rfl) (Stage.le_refl _) .refl))
      (typ_con0 (us := []) rfl))
    (fun s h => by simp [caseSeq, Flick.Tm.Has] at h))
    (fun s h => by simp [caseSeq, Flick.Tm.Has] at h)

/-- `f x` and the call of `each_φ f xs` both need `C` live, as the stage of the body has it. -/
theorem typ_seqEach {A D C C0 ψ φ} :
    Typ Sg Ψ Θ [] seqEach (.ex (.arrow (.arrow A ψ (tc "Unit" []) C) .pure
      (.arrow (seqT D A) ψ (tc "Unit" []) C) C0) φ) :=
  typ_seqEach_at (Stage.le_refl _)

theorem typ_seqFlatMap {A B C C2 C0 D E R φ} :
    Typ Sg Ψ Θ [] seqFlatMap (.ex (.arrow (.arrow A .pure (seqT E B) C) .pure
      (.arrow (seqT D A) .pure (seqT R B) C2) C0) φ) :=
  typ_lam (typ_fix (typ_caseSeq (typ_var_mono rfl)
      (.let_ (.app (ψ := .pure) (typ_var_mono rfl) (typ_var_mono rfl) trivial .refl)
        (.app (typ_fix (ψ := .pure) (C := Caps.none) (typ_caseSeq (typ_var_mono rfl)
            (typ_scons (typ_var_mono rfl) (.app (ψ := .pure) (typ_var_mono rfl) (typ_var_mono rfl) trivial .refl))
            (typ_var_mono rfl))
          (fun s h => by simp [seqApp, caseSeq, scons, Flick.Tm.Has] at h))
          (.app (ψ := .pure) (typ_var_mono rfl) (typ_var_mono rfl) trivial .refl) trivial .refl))
      typ_snil)
    (fun s h => by simp [seqApp, caseSeq, scons, snil, Flick.Tm.Has] at h))
    (fun s h => by simp [seqApp, caseSeq, scons, snil, Flick.Tm.Has] at h)

end Flick.Elab.Witness
