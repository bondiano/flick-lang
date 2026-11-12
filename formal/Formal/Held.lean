import Formal.SubAdm

/-!
# Shapes of the types of values

What a constructor spine, a partial application of a constant and a subtype show of
their result types (`Ty.res`, `Ty.Shows`), that a typed spine is not a `∀` type, and that
a typed value has no instance forms (`Typ.noIns`). Instantiating a `∀` (`AllElim.lean`)
uses them.
-/

namespace Flick

variable {S : Sig}

theorem Sig.impl_reach (S : Sig) (N : Name) (a : Atom) : ¬ (S.impl N).reach a := by
  intro h
  rw [← S.impl_closed N (fun _ => .base .unit), Ty.reach_substT] at h
  obtain ⟨_, _, h⟩ := h
  exact h

/-- The result after all stages of an arrow. -/
def Ty.res : Ty → Ty
  | .arrow _ _ t _ => t.res
  | t => t

/-- Each argument of a chain of arrows is shown by its final result. -/
def Ty.Shows : Ty → Prop
  | .arrow a _ t _ => Caps.Sub a.reach t.res.reach ∧ t.Shows
  | _ => True

theorem Ty.res_reach (t : Ty) : Caps.Sub t.res.reach t.reach := by
  induction t with
  | arrow _ _ _ _ _ ih => exact fun a h => .inr (.inl (ih a h))
  | _ => exact Caps.Sub.refl _

theorem Ty.arrows_res (fs : List Ty) (r : Ty) (hr : ∀ a φ t C, r ≠ .arrow a φ t C) :
    (Ty.arrows fs r).res = r := by
  induction fs with
  | nil =>
    cases r with
    | arrow => exact absurd rfl (hr _ _ _ _)
    | _ => rfl
  | cons _ _ ih => exact ih

theorem Ty.tcon_not_arrow (T : Name) (us : List Ty) : ∀ a φ t C, Ty.tcon T us ≠ .arrow a φ t C := by
  have : ∀ (us : List Ty) (f : Ty), (∀ a φ t C, f ≠ .arrow a φ t C) →
      ∀ a φ t C, us.foldl .tapp f ≠ .arrow a φ t C := by
    intro us
    induction us with
    | nil => intro f h; exact h
    | cons x xs ih => intro f _; exact ih _ (by intros; simp)
  exact this us _ (by intros; simp)

theorem Ty.shows_of {t : Ty} (h : ∀ a φ b C, t ≠ .arrow a φ b C) : t.Shows := by
  cases t with
  | arrow => exact absurd rfl (h _ _ _ _)
  | _ => trivial

theorem Sig.ctorTyR_res (ts : List Ty) (T : Name) (rs us : List Ty) :
    (Sig.ctorTyR ts T rs us).res = Ty.tcon T (rs.map (Ty.substT (instT us))) :=
  Ty.arrows_res _ _ (Ty.tcon_not_arrow T _)

/-- A constructor's type shows its fields when they hold no more than its value. -/
theorem Sig.ctorTyR_shows {ts : List Ty} {T : Name} {rs us : List Ty}
    (hH : Sig.Holds ts T rs us) : (Sig.ctorTyR ts T rs us).Shows := by
  have key : ∀ fs : List Ty, fs ⊆ ts →
      (Ty.arrows (fs.map (Ty.substT (instT us))) (Ty.tcon T (rs.map (Ty.substT (instT us))))).Shows := by
    intro fs
    induction fs with
    | nil => intro _; exact Ty.shows_of (Ty.tcon_not_arrow T _)
    | cons f fs ih =>
      intro hfs
      refine ⟨?_, ih fun x hx => hfs (List.mem_cons_of_mem _ hx)⟩
      rw [Ty.arrows_res _ _ (Ty.tcon_not_arrow T _)]
      exact hH f (hfs (List.mem_cons_self ..))
  exact key ts (List.Subset.refl _)

theorem Sig.ctorTy_shows (S : Sig) {K k ts T} (hK : S.ctor K = some (k, ts, T)) {us : List Ty}
    (hk : us.length = k) : (Sig.ctorTy ts T us).Shows ∧
      (Sig.ctorTy ts T us).res = Ty.tcon T us := by
  have hres := Ty.arrows_res (ts.map (Ty.substT (instT us))) _ (Ty.tcon_not_arrow T us)
  refine ⟨?_, hres⟩
  have key : ∀ fs : List Ty, fs ⊆ ts → (Ty.arrows (fs.map (Ty.substT (instT us))) (Ty.tcon T us)).Shows := by
    intro fs
    induction fs with
    | nil => intro _; exact Ty.shows_of (Ty.tcon_not_arrow T us)
    | cons f fs ih =>
      intro hfs
      refine ⟨?_, ih fun x hx => hfs (List.mem_cons_of_mem _ hx)⟩
      rw [Ty.arrows_res _ _ (Ty.tcon_not_arrow T us)]
      exact S.field_reach hK hk f (hfs (List.mem_cons_self ..))
  exact key ts (List.Subset.refl _)

/-- What a constant with one argument shows of it. -/
theorem Const.shows_first (c : Const) (ι : CArg) (S : Sig) (hc : c.arity = 2) :
    ∃ a t, c.ty S ι = .arrow a .pure t Caps.none ∧ Caps.Sub a.reach t.reach := by
  cases c <;> simp [Const.arity] at hc <;>
    refine ⟨_, _, rfl, ?_⟩ <;> intro x h <;>
    simp_all [Ty.reach, Caps.none]

theorem Ty.tcon_hd (T : Name) (us : List Ty) : (Ty.tcon T us).hd = .adt T := by
  have : ∀ (us : List Ty) (f : Ty), f.hd = .adt T → (us.foldl .tapp f).hd = .adt T := by
    intro us; induction us with
    | nil => intro f h; exact h
    | cons x xs ih => intro f h; exact ih _ h
  exact this us _ rfl

theorem Ty.not_all_of_hd {U : Ty} {E : Name} (h : U.hd = .adt E) : ¬ U.isAll := by
  cases U <;> simp_all [Ty.hd, Ty.isAll]

theorem Ty.res_tcon_not_all {k : Nat} {t : Ty} {T : Name} {us : List Ty}
    (h : (Ty.all k t).res = Ty.tcon T us) : False :=
  Ty.not_all_of_hd (Ty.tcon_hd T us) (h ▸ trivial)

theorem Ty.res_tcon_self (T : Name) (us : List Ty) : (Ty.tcon T us).res = Ty.tcon T us := by
  have := Ty.tcon_isData T us
  generalize Ty.tcon T us = t at this ⊢
  cases t <;> simp_all [Ty.isData, Ty.res]

theorem LeLive.res {a b : Ty} (h : LeLive S a b) : LeLive S a.res b.res := by
  induction h with
  | refl => exact .refl
  | arrow _ _ _ _ _ ih => exact ih
  | eff hφ ht hc => exact .eff hφ ht hc
  | hide hi hl he hr ha hb =>
    subst ha; subst hb
    rw [Ty.res_tcon_self, Ty.res_tcon_self]
    exact .hide hi hl he hr rfl rfl

theorem Ty.tcon_not_isAll (T : Name) (us : List Ty) : ¬ (Ty.tcon T us).isAll := by
  have := Ty.tcon_isData T us
  generalize Ty.tcon T us = t at this ⊢
  cases t <;> simp_all [Ty.isData, Ty.isAll]

theorem LeLive.not_all {x y : Ty} (h : LeLive S x y) (hx : ¬ x.isAll) : ¬ y.isAll := by
  cases h with
  | refl => exact hx
  | arrow | eff => simp [Ty.isAll]
  | hide _ _ _ _ _ hb => subst hb; exact Ty.tcon_not_isAll _ _

theorem LeLiveI.snoc {a k t} {us : List Ty} (h : LeLiveI S a (.all k t)) (hus : us.length = k) :
    LeLiveI S a (t.substT (instT us)) := by
  generalize ht : Ty.all k t = T at h
  induction h with
  | le h => subst ht; obtain rfl := h.eq_plain_right rfl; exact .inst hus .refl
  | inst hus' _ ih => exact .inst hus' (ih ht)

/-- Only a binder is below a binder. -/
theorem LeLive.eq_all {x : Ty} {k t}
    (h : LeLive S x (.all k t)) : x = .all k t :=
  h.eq_plain_right rfl

/-- Subtyping keeps the shape of an arrow and relates its results. -/
theorem LeLive.arrow_res {a φ t C T : _}
    (h : LeLive S (.arrow a φ t C) T) :
    ∃ a' φ' t' C', T = .arrow a' φ' t' C' ∧ LeLive S t t' := by
  obtain ⟨_, _, _, _, he, _, _, ht, -⟩ := LeLive.arrow_left h; exact ⟨_, _, _, _, he, ht⟩

theorem Const.ty_isArrow (c : Const) (ι : CArg) (S : Sig) :
    ∃ a φ t C, c.ty S ι = .arrow a φ t C := by
  cases c
  case perform E o _ =>
    simp only [Const.ty]
    split <;> exact ⟨_, _, _, _, rfl⟩
  case op => exact ⟨_, _, _, _, rfl⟩
  all_goals exact ⟨_, _, _, _, rfl⟩

/-- The result of a constructor spine is not a binder, unless the value itself is one. -/
theorem Typ.spine_notAll {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    Value e → ∀ {K T φ}, e.head = .ctor K → J = .ex T φ → T.isAll ∨ ¬ T.res.isAll := by
  induction h with
  | allI => intro _ _ _ _ _ hJ; cases hJ; exact .inl trivial
  | errVal => intro _ _ _ _ _ hJ; cases hJ; exact .inr (by simp [Ty.res, Ty.isAll])
  | ctor hK hk hH hT =>
    intro _ _ _ _ _ hJ; cases hJ
    refine .inr (LeLive.not_all hT.res ?_)
    rw [Sig.ctorTyR, Ty.arrows_res _ _ (Ty.tcon_not_arrow _ _)]
    exact Ty.not_all_of_hd (Ty.tcon_hd _ _)
  | app hf _ _ hT ih₁ _ =>
    intro hv _ _ _ hK hJ; cases hJ
    cases hv with
    | ctorApp hvf hK' _ =>
      rcases ih₁ hvf hK' rfl with h' | h'
      · exact h'.elim
      · exact .inr (LeLive.not_all hT.res h')
    | constApp => simp [Tm.head] at hK
  | _ => intro hv _ _ _ hK hJ; cases hJ <;> cases hv <;> simp [Tm.head] at hK

theorem Caps.Sub.of_none {C : Caps} (D : Caps) (h : Caps.Sub C Caps.none) : Caps.Sub C D :=
  fun a ha => (h a ha).elim

/-- A value has no `inside` form. -/
theorem Typ.noIns {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) : Value e → e.NoIns := by
  induction h with
  | lam _ hn => intro _; exact hn
  | fix _ hn => intro _; exact hn
  | task _ _ hn => intro _; exact hn
  | rcons _ _ _ _ ih₁ ih₂ =>
    intro hv; cases hv with
    | rcons hv₁ hv₂ =>
      rintro (h | h)
      · exact ih₁ hv₁ h
      · exact ih₂ hv₂ h
  | errVal hv _ _ _ ih | allI hv _ ih => intro _; exact ih hv
  | app _ _ _ _ ih₁ ih₂ =>
    intro hv
    cases hv with
    | ctorApp hvf _ hve =>
      rintro (h | h)
      · exact ih₁ hvf h
      · exact ih₂ hve h
    | constApp _ hve =>
      rintro (h | h)
      · exact h
      · exact ih₂ hve h
  | lit | loc | const | ctor | rnil | cap | own | dnil | perform => intro _ h; cases h
  | kont => intro _ h; simp [Tm.Has] at h
  | drec _ _ _ hn => intro _; exact hn
  | dcons _ _ ih₁ ih₂ =>
    intro hv; cases hv with
    | rcons hv₁ hv₂ =>
      rintro (h | h)
      · exact ih₁ hv₁ h
      · exact ih₂ hv₂ h
  | _ => intro hv; cases hv

end Flick
