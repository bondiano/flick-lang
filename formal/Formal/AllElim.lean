import Formal.TySubst
import Formal.Held

/-!
# Instances of a polymorphic value

A value of type `∀ᾱ. τ` has `τ` with `ᾱ` fresh, and so each instance of `τ`. The term
that a variable stands for in the substitution lemma is a value or another variable, so
(Var) at an instance survives the substitution without a rule for any other term.
-/

namespace Flick

variable {S : Sig}

/-- What a variable can stand for: a value, a variable, or `todo` for what a dead arm
does not use. -/
def Tm.Stands (e : Tm) : Prop := Value e ∨ (∃ j, e = .var j) ∨ e = .todo

/-- `e` stands for a variable of scheme `s`, in the context `Γ` of `e`: if a variable, then
one of the same scheme. A premise of a dictionary type can stand for any term of that type,
such as a global, as no instance of the type is a binder. -/
def Tm.StandsAt (Γ : List Scheme) (s : Scheme) (e : Tm) : Prop :=
  Value e ∨ (∃ j, e = .var j ∧ Γ[j]? = some s) ∨ e = .todo ∨ (s.k = 0 ∧ ∃ c u, s.t = .dict c u)

theorem Tm.StandsAt.stands {Γ s} {e : Tm} (h : e.StandsAt Γ s) (ts : List Ty) :
    e.Stands ∨ ¬ (s.inst ts).isAll := by
  rcases h with h | ⟨j, hj, _⟩ | h | ⟨_, _, _, hu⟩
  · exact .inl (.inl h)
  · exact .inl (.inr (.inl ⟨j, hj⟩))
  · exact .inl (.inr (.inr h))
  · exact .inr (by simp [Scheme.inst, hu, Ty.substT, Ty.isAll])

theorem Tm.StandsAt.substT {Γ s} {e : Tm} (h : e.StandsAt Γ s) (θ : Atom → Ty) :
    e.StandsAt (Γ.map (Scheme.substT θ)) (s.substT θ) := by
  rcases h with h | ⟨j, hj, hs⟩ | h | ⟨hk, c, u, hu⟩
  · exact .inl h
  · exact .inr (.inl ⟨j, hj, by simp [hs]⟩)
  · exact .inr (.inr (.inl h))
  · exact .inr (.inr (.inr ⟨hk, c, u.substT (liftT s.k θ), by simp [Scheme.substT, hu, Ty.substT]⟩))

theorem Tm.StandsAt.rename {Γ Γ' s} {e : Tm} {ξ : Nat → Nat} (h : e.StandsAt Γ s) (hξ : RenOk Γ Γ' ξ) :
    (e.rename ξ).StandsAt Γ' s := by
  rcases h with h | ⟨j, rfl, hs⟩ | rfl | h
  · exact .inl h.rename
  · exact .inr (.inl ⟨_, rfl, hξ _ _ hs⟩)
  · exact .inr (.inr (.inl rfl))
  · exact .inr (.inr (.inr h))

private theorem Typ.constResult {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ c a ψ t C φ, e = .const c → c.arity = 2 → J = .ex (.arrow a ψ t C) φ → ¬ t.isAll := by
  induction h with
  | conv _ he ih =>
    intro c a ψ t C φ hc hArity hJ; cases hJ
    cases he.symm.form with
    | rows hr _ => cases hr
    | arrow _ ht =>
      have hn := ih c _ _ _ _ _ hc hArity rfl
      rw [ht.isAll]
      exact hn
  | const _ hT =>
    intro c a ψ t C φ hc hArity hJ; cases hc; cases hJ
    have key : ∀ (c : Const) ι, c.arity = 2 →
        ∃ x y, c.ty S ι = .arrow x .pure y Caps.none ∧ ¬ y.isAll := by
      intro c _ hc
      cases c <;> simp [Const.arity] at hc <;> exact ⟨_, _, rfl, by simp [Ty.isAll]⟩
    obtain ⟨_, _, hxy, hy⟩ := key _ _ hArity
    rw [hxy] at hT
    obtain ⟨_, _, _, _, he, ht⟩ := hT.arrow_res
    cases he
    exact ht.not_all hy
  | perform => intro c a ψ t C φ hc hArity _; cases hc; simp [Const.arity] at hArity
  | allI | errVal => intro c a ψ t C φ _ _ hJ; cases hJ
  | _ => intro c a ψ t C φ hc _ _; cases hc

private theorem Typ.allInvCore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    Value e → ∀ k t φ, J = .ex (.all k t) φ →
      Typ S (Ψ.map (Ty.up k)) (Θ.substT (shiftV k)) (Γ.map (Scheme.up k)) e (.ex t .pure) := by
  have noArrow : ∀ {a φ t C T}, LeLive S (.arrow a φ t C) T → ¬ T.isAll := fun h' => by
    obtain ⟨_, _, _, _, rfl, -⟩ := h'.arrow_res; simp [Ty.isAll]
  have noEff : ∀ {r ε t C T}, LeLive S (.eff r ε t C) T → ¬ T.isAll := fun h' => by
    obtain ⟨_, _, _, _, rfl, -⟩ := h'.eff_left; simp [Ty.isAll]
  induction h with
  | conv _ he ih =>
    intro hv k t φ hJ; cases hJ
    cases he.symm.form with
    | rows hr _ => cases hr
    | all ht => exact (ih hv _ _ _ rfl).conv ht.symm
  | allI _ ht => intro _ k t φ hJ; cases hJ; exact ht
  | lam _ _ hT | fix _ _ hT | perform _ hT | kont _ _ hT =>
    intro _ k t φ hJ; cases hJ; exact (noArrow hT trivial).elim
  | task _ _ _ hT => intro _ k t φ hJ; cases hJ; exact (noEff hT trivial).elim
  | const _ hT =>
    intro _ k t φ hJ; cases hJ
    obtain ⟨_, _, _, _, hx⟩ := Const.ty_isArrow _ _ S
    rw [hx] at hT; exact (noArrow hT trivial).elim
  | ctor _ _ _ hT =>
    intro _ k t φ hJ; cases hJ
    have hres := hT.res
    rw [Sig.ctorTyR, Ty.arrows_res _ _ (Ty.tcon_not_arrow _ _)] at hres
    exact (hres.not_all (Ty.not_all_of_hd (Ty.tcon_hd _ _)) trivial).elim
  | app hf _ _ hT =>
    intro hv k t φ hJ; cases hJ
    cases hv with
    | ctorApp hvf hK _ =>
      rcases Typ.spine_notAll hf hvf hK rfl with h | h
      · exact h.elim
      · exact (hT.res.not_all h trivial).elim
    | constApp hc _ => exact (hT.not_all (Typ.constResult hf _ _ _ _ _ _ rfl hc rfl) trivial).elim
  | _ => intro hv k t φ hJ; cases hJ <;> cases hv

/-- A value of a binder's type comes from (∀I), possibly followed by type conversion. -/
theorem Typ.allInv {Ψ Θ Γ e k t φ} (hv : Value e) (h : Typ S Ψ Θ Γ e (.ex (.all k t) φ)) :
    Typ S (Ψ.map (Ty.up k)) (Θ.substT (shiftV k)) (Γ.map (Scheme.up k)) e (.ex t .pure) :=
  h.allInvCore hv k t φ rfl

theorem Facts.up_substT (Θ : Facts) (k : Nat) (ts : List Ty) (h : ts.length = k) :
    (Θ.substT (shiftV k)).substT (instT ts) = Θ := by
  rw [Facts.substT_comp]; conv => rhs; rw [← Θ.substT_ident]
  congr 1; funext a
  cases a with
  | v i =>
    have : ¬ i + k < ts.length := by omega
    simp only [shiftV, Ty.substT, instT, dif_neg this, Ty.ident]
    congr 1; omega
  | r p => simp [shiftV, Ty.substT, instT, Ty.ident]

private theorem Typ.instVarCore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ j k t φ (us : List Ty), e = .var j → J = .ex (.all k t) φ → us.length = k →
      Typ S Ψ Θ Γ (.var j) (.ex (t.substT (instT us)) .pure) := by
  induction h with
  | conv _ he ih =>
    intro j k t φ us hj hJ hus; cases hJ
    cases he.symm.form with
    | rows hr _ => cases hr
    | all ht => exact (ih j k _ _ us hj rfl hus).conv (ht.symm.substT (instT us))
  | var hi hts hH hT =>
    intro j k t φ us hj hJ hus; cases hj; cases hJ
    exact .var hi hts hH (hT.snoc hus)
  | allVar hi _ hc =>
    intro j k t φ us hj hJ hus; cases hj; cases hJ
    exact .var hi hus (fun _ _ h => (hc _ _ h).elim) LeLiveI.refl
  | allI hv => intro j k t φ us hj _ _; cases hj; cases hv
  | errVal => intro j k t φ us _ hJ _; cases hJ
  | _ => intro j k t φ us hj _ _; cases hj

/-- One instance of a binder. -/
theorem Typ.instAll {Ψ Θ Γ e k t φ} {us : List Ty} (hV : e.Stands) (hn : e.NoIns)
    (h : Typ S Ψ Θ Γ e (.ex (.all k t) φ)) (hus : us.length = k) :
    Typ S Ψ Θ Γ e (.ex (t.substT (instT us)) .pure) := by
  rcases hV with hv | ⟨j, rfl⟩ | rfl
  · have := (Typ.allInv hv h).substT (instT us) (Fits.noIns hn fun _ => rfl)
    simp only [List.map_map, Function.comp_def, Ty.up_substT _ _ _ hus,
      Scheme.up_substT _ _ _ hus, List.map_id', Flick.J.substT, Facts.up_substT _ _ _ hus] at this
    exact this
  · exact h.instVarCore j k t φ us rfl rfl hus
  · exact .todo

/-- (Var) at an instance, for the term that stands for the variable. -/
theorem Typ.elim {Ψ Θ Γ e A T} (hV : e.Stands ∨ ¬ A.isAll) (hn : e.NoIns)
    (h : Typ S Ψ Θ Γ e (.ex A .pure)) (hT : LeLiveI S A T) :
    Typ S Ψ Θ Γ e (.ex T .pure) := by
  induction hT with
  | le hl => exact h.sub ⟨hl, Stage.le_refl _⟩
  | inst hus _ ih =>
    have hs := hV.resolve_right fun h => h trivial
    exact ih (.inl hs) (Typ.instAll hs hn h hus)

end Flick
