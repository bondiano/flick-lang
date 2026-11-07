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

/-- A value of a binder's type is typed by (∀I): no other rule gives a value that type. -/
theorem Typ.allInv {Ψ Θ Γ e k t φ} (hv : Value e) (h : Typ S Ψ Θ Γ e (.ex (.all k t) φ)) :
    Typ S (Ψ.map (Ty.up k)) (Θ.substT (shiftV k)) (Γ.map (Scheme.up k)) e (.ex t .pure) := by
  cases h with
  | allI _ h => exact h
  | lam _ _ _ hT | fix _ _ _ hT | perform _ hT | kont _ _ hT | task _ _ _ _ hT => cases hT
  | const _ hT =>
    obtain ⟨_, _, _, _, hx⟩ := Const.ty_isArrow _ _ S
    rw [hx] at hT; cases hT
  | ctor _ _ _ hT =>
    have := hT.res
    rw [Sig.ctorTyR, Ty.arrows_res _ _ (Ty.tcon_not_arrow _ _)] at this
    exact (Le.not_all this (Ty.not_all_of_hd (Ty.tcon_hd _ _)) trivial).elim
  | app hf _ _ hT =>
    cases hT
    cases hv with
    | ctorApp hvf hK _ =>
      rcases Typ.spine_notAll hf hvf hK rfl with h | h
      · exact h.elim
      · exact (h trivial).elim
    | constApp hc _ =>
      cases hf with
      | const _ hT' =>
        have key : ∀ (c : Const) ι, c.arity = 2 →
            ∃ x y, c.ty S ι = .arrow x .pure y Caps.none ∧ ¬ y.isAll := by
          intro c _ hc
          cases c <;> simp [Const.arity] at hc <;> exact ⟨_, _, rfl, by simp [Ty.isAll]⟩
        obtain ⟨_, _, hxy, hy⟩ := key _ _ hc
        rw [hxy] at hT'
        obtain ⟨_, _, _, _, he, _, _, lt, -⟩ := hT'.arrow_left
        cases he; cases lt; exact (hy trivial).elim
      | perform => simp [Const.arity] at hc
  | _ => cases hv

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

/-- One instance of a binder. -/
theorem Typ.instAll {Ψ Θ Γ e k t φ} {us : List Ty} (hV : e.Stands) (hn : e.NoIns)
    (h : Typ S Ψ Θ Γ e (.ex (.all k t) φ)) (hus : us.length = k) :
    Typ S Ψ Θ Γ e (.ex (t.substT (instT us)) .pure) := by
  rcases hV with hv | ⟨j, rfl⟩ | rfl
  · have := (Typ.allInv hv h).substT (instT us) (Fits.noIns hn fun _ => rfl)
    simp only [List.map_map, Function.comp_def, Ty.up_substT _ _ _ hus,
      Scheme.up_substT _ _ _ hus, List.map_id', Flick.J.substT, Facts.up_substT _ _ _ hus] at this
    exact this
  · cases h with
    | var hi hts hH hT => exact .var hi hts hH (hT.snoc hus)
    | allVar hi _ hc => exact .var hi hus (fun _ _ h => (hc _ _ h).elim) .refl
    | allI hv => cases hv
  · exact .todo

/-- (Var) at an instance, for the term that stands for the variable. -/
theorem Typ.elim {Ψ Θ Γ e A T} (hV : e.Stands ∨ ¬ A.isAll) (hn : e.NoIns)
    (h : Typ S Ψ Θ Γ e (.ex A .pure)) (hT : LeI A T) : Typ S Ψ Θ Γ e (.ex T .pure) := by
  induction hT with
  | le hl => exact h.sub ⟨hl, Stage.le_refl _⟩
  | inst hus _ ih =>
    have hs := hV.resolve_right fun h => h trivial
    exact ih (.inl hs) (Typ.instAll hs hn h hus)

end Flick
