import Formal.ScopeStep

/-!
# `Scope.run`

`Scope.run : ∀r e a. (∀s. Owner s →R⟨Scope s | r; e⟩ a) →R⟨r; e⟩ a`, for a function of any
form, not only a lambda: it is a `scope` whose body gives that function the owner.
-/

namespace Flick

variable {S : Sig}

theorem Row.obs_ty (a : Atom) (r : Row) : Row.obs r a.ty = Row.disch a r := by
  funext x; apply propext
  cases x <;> simp [Row.obs, Row.disch, Atom.ty_reach, and_comm]

/-- The step of `Scope.run f` keeps the type, for a closed value `f`. In the live discipline
the qualifier of `f` is live in the rows around the scope (`Const.ok` of `.scopeRun`). -/
theorem scopeRun_body {Ψ Θ f t T φ φ₀} {r ε : Row} {C : Caps} (hv : Value f)
    (h : Typ S Ψ Θ [] f (.ex (.all 1 (.arrow (.owner (.tvar 0))
      (.rt (Row.union (Row.scope (.tvar 0)) (Row.union (r.substT (shiftV 1))
        (Row.obs (r.substT (shiftV 1)) (.tvar 0)))) (ε.substT (shiftV 1)))
      (t.up 1) (C.substT (shiftV 1)))) φ₀))
    (hφ : (Stage.rt r ε).le φ) (hT : LeLive S t T)
    (hC : Row.Sub (Row.ofCaps C) r) :
    Typ S Ψ Θ [] (.scope (.app (f.rename (· + 1)) (.var 0))) (.ex T φ) := by
  have hr := (Typ.allInv hv h).rename (Γ' := [.mono (.owner (.tvar 0))]) (ξ := (· + 1))
    fun _ _ hj => by simp at hj
  have hobs := Row.obs_ty (.v 0) (r.substT (shiftV 1))
  simp only [Atom.ty] at hobs
  refine .scope (F := Facts.none) (φ' := .rt (Row.union (Row.scope (.tvar 0))
    (Row.union (r.substT (shiftV 1)) (Row.obs (r.substT (shiftV 1)) (.tvar 0)))) (ε.substT (shiftV 1)))
    ?_ (fun _ _ h => h.elim) ?_ ?_ hφ hT
  · rintro (h' | h')
    · exact (h.noIns hv).rename _ h'
    · cases h'
  · refine (Typ.app (hr.sub ⟨LeLive.refl, Stage.pure_le _⟩)
      (.var (σ := .mono (.owner (.tvar 0))) (ts := []) rfl rfl
        (fun _ _ h => False.elim h) LeLiveI.refl) ?_ LeLive.refl).cut
      (Entails.union_left _ _)
    have hs := hC.substT (shiftV 1)
    rw [Row.ofCaps_substT] at hs
    exact ⟨fun x hx => hx.elim id fun h' => .inr (.inl (hs x h')), fun _ h' => h'⟩
  · rw [← hobs]; exact Stage.le_refl _

/-- `Scope.run (λ. b)` and `scope b` run the same way: each creates the region at the end of
the regions, leaves the store alone and runs `b` with its owner, inside the region. -/
theorem scopeRun_agrees {root : Roots} {Q : Lbl → Prop} (hrt : Q .rt) (hp : Q .pure) {Λ : List Rgn}
    {μ : List Tm} {b : Tm} (hb : (Tm.lam b).Closed) :
    Steps root Q Λ μ (.scope b) (Λ ++ [Rgn.new]) μ (.inside Λ.length (b.subst (inst [.own Λ.length]))) ∧
    Steps root Q Λ μ (.app (.const .scopeRun) (.lam b)) (Λ ++ [Rgn.new]) μ
      (.inside Λ.length (b.subst (inst [.own Λ.length]))) := by
  have hs : ∀ {e : Tm}, Step root .rt Λ μ (.scope e) (.cfg (Λ ++ [Rgn.new]) μ
      (.inside Λ.length (e.subst (inst [.own Λ.length])))) := Step.ofR .scope
  have hinst : (Tm.app ((Tm.lam b).rename (· + 1)) (.var 0)).subst (inst [.own Λ.length]) =
      .app (.lam b) (.own Λ.length) := by
    simp only [Tm.subst]
    rw [hb.rename, hb.subst]; rfl
  refine ⟨.step hs hrt .refl, .step (Step.ofR (.scopeRun .lam)) hrt (.step hs hrt ?_)⟩
  rw [hinst]
  exact .step (Step.pure (E := [.inside Λ.length]) (fun F hF => by simp at hF; subst hF; trivial)
    (.beta .own)) hp .refl

/-- A `scope` with no facts about its region types as `Scope.run` of a lambda. A body that uses
a fact has no such typing: an instance of the binder may be any region. -/
theorem Typ.scopeRun_of_scope {Ψ Θ Γ b t T φ φ'} {r ε : Row} (hn : b.NoIns)
    (hb : Typ S (Ψ.map (Ty.up 1)) (Facts.union (Θ.substT (shiftV 1)) Facts.none)
      (.mono (.owner (.tvar 0)) :: Γ.map (Scheme.up 1)) b (.ex (t.up 1) φ'))
    (hφ' : φ'.le (.rt (Row.union (Row.scope (.tvar 0))
      (Row.union (r.substT (shiftV 1)) (Row.disch (.v 0) (r.substT (shiftV 1))))) (ε.substT (shiftV 1))))
    (hφ : (Stage.rt r ε).le φ) (hT : LeLive S t T) :
    Typ S Ψ Θ Γ (.app (.const .scopeRun) (.lam b)) (.ex T φ) := by
  have hobs := Row.obs_ty (.v 0) (r.substT (shiftV 1))
  simp only [Atom.ty] at hobs
  rw [← hobs] at hφ'
  -- the lambda uses nothing but its rows: its qualifier is empty
  refine .app (.const (c := .scopeRun) (ι := ⟨t, .base .unit, Caps.none, r, ε⟩)
    (fun x h => by cases x <;> exact h.elim) .refl) (.allI .lam (.lam ((hb.cut Entails.union_none).sub
      ⟨.refl, Stage.le_trans hφ' (Stage.le_use _ _)⟩) hn .refl)) (by rw [Stage.use_none]; exact hφ) hT

/-- `Scope.run` of a lambda types as a `scope` of its body. -/
theorem Typ.scope_of_scopeRun {Ψ Θ Γ b T φ} (h : Typ S Ψ Θ Γ (.app (.const .scopeRun) (.lam b)) (.ex T φ)) :
    Typ S Ψ Θ Γ (.scope b) (.ex T φ) := by
  cases h with
  | app hf ha hψ hT =>
    cases hf with
    | const hok hT' =>
      obtain ⟨_, _, _, _, h, la, lψ, lt, -⟩ := hT'.arrow_left
      cases h; obtain rfl := la.eq_plain_right rfl
      have hs := hok.substT (shiftV 1)
      rw [Row.ofCaps_substT] at hs
      cases Typ.allInv .lam ha with
      | lam hb hn hL =>
        obtain ⟨_, _, _, _, h, la', lψ', lt', -⟩ := hL.arrow_left
        cases h; obtain rfl := la'.eq_plain rfl
        rw [show ∀ x, Row.obs x (.tvar 0) = Row.disch (.v 0) x from Row.obs_ty (.v 0)] at lψ'
        exact .scope (F := Facts.none) hn (fun _ _ h => h.elim)
          ((hb.sub (J' := .ex _ _) ⟨lt', Stage.le_refl _⟩).cut (Entails.union_left _ _))
          (Stage.le_trans lψ' ⟨fun x hx => hx.elim id fun h' => .inr (.inl (hs x h')), fun _ h' => h'⟩)
          (by simpa only [Stage.use_none] using Stage.le_trans lψ hψ) (lt.trans hT)
  | errVal hv | allI hv => cases hv with
    | ctorApp _ hK _ => simp [Tm.head] at hK
    | constApp hc _ => simp [Const.arity] at hc

end Flick
