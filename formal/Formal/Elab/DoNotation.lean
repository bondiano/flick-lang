import Formal.Elab.SugarBase

/-!
# Elaboration: `do` (elaboration.md §3.1)

`DoOk` is the checker's typing of a `do` block (`do_notation.rs`, `applicative_do.rs`): the
rest of a block after `<-` is the pure continuation of `flat_map`, a guard needs
`Alternative m`, a group `and` needs `Applicative m` and ends either in `pure e` (`map2`
alone) or in the rest (`flat_map` over the tuple). `do_elab`: the desugared block elaborates
with the block's type; `T6a_do`: T6(a) covers it.
-/

namespace Flick.Elab

variable {P : Prog} (S : Sugar P)

/-- The typing of `do { s̄; r }` at the stage `φ`. -/
inductive DoOk : List Scheme → List Cn → List Stmt → Exp → Ty → Stage → Prop
  | ret {Γ Δ r τ φ r'} : Elab P Γ Δ r τ φ r' → DoOk Γ Δ [] r τ φ
  | bind {Γ Δ e ss r m a b φ d e'} : Ent P Δ [⟨S.monad, [m]⟩] [d] →
      Elab P Γ Δ e (.app m a) φ e' → DoOk (Scheme.mono a :: Γ) Δ ss r (.app m b) .P →
      DoOk Γ Δ (.bind e :: ss) r (.app m b) φ
  | exp {Γ Δ e ss r m a b φ d e'} : Ent P Δ [⟨S.monad, [m]⟩] [d] →
      Elab P Γ Δ e (.app m a) φ e' → DoOk Γ Δ ss r (.app m b) .P →
      DoOk Γ Δ (.exp e :: ss) r (.app m b) φ
  | let_ {Γ Δ e ss r a τ φ e'} : Elab P Γ Δ e a φ e' → DoOk (Scheme.mono a :: Γ) Δ ss r τ φ →
      DoOk Γ Δ (.let_ e :: ss) r τ φ
  | letSig {Γ Δ σ e ss r τ φ₁ φ e'} :
      Elab P (Γ.map (·.subst (up σ.k))) (σ.ctx ++ Δ.map (·.subst (up σ.k))) e σ.ty φ₁ e' →
      (σ.ctx ≠ [] ∨ σ.k ≠ 0 → φ₁ = .P) → (φ₁ = .P ∨ φ₁ = φ) →
      (σ.ctx ≠ [] → ∀ n, heldS (Γ.map (·.subst (up σ.k))) e n → σ.ty.reach n ∨ σ.q.reach n) →
      DoOk (σ :: Γ) Δ ss r τ φ →
      DoOk Γ Δ (.letSig σ e :: ss) r τ φ
  | guard {Γ Δ c ss r m b φ d c'} : Ent P Δ [⟨S.alternative, [m]⟩] [d] →
      Elab P Γ Δ c boolTy φ c' → DoOk Γ Δ ss r (.app m b) φ →
      DoOk Γ Δ (.guard c :: ss) r (.app m b) φ
  | group {Γ Δ e₁ mid eₙ ss r m a₁ as aₙ b φ d d' e₁' eₙ'} : S.pureLast ss r = none →
      Ent P Δ [⟨S.monad, [m]⟩] [d] → Ent P Δ [⟨S.applicative, [m]⟩] [d'] →
      Elab P Γ Δ e₁ (.app m a₁) φ e₁' →
      All2 (fun e a => ∃ e', Elab P Γ Δ e (.app m a) φ e') mid as →
      Elab P Γ Δ eₙ (.app m aₙ) φ eₙ' →
      DoOk ((a₁ :: as ++ [aₙ]).reverse.map Scheme.mono ++ Γ) Δ ss r (.app m b) .P →
      DoOk Γ Δ (.group e₁ mid eₙ :: ss) r (.app m b) φ
  | groupPure {Γ Δ e₁ mid eₙ ss r v m a₁ as aₙ x φ d' e₁' eₙ' v'} : S.pureLast ss r = some v →
      Ent P Δ [⟨S.applicative, [m]⟩] [d'] →
      Elab P Γ Δ e₁ (.app m a₁) φ e₁' →
      All2 (fun e a => ∃ e', Elab P Γ Δ e (.app m a) φ e') mid as →
      Elab P Γ Δ eₙ (.app m aₙ) φ eₙ' →
      Elab P ((a₁ :: as ++ [aₙ]).reverse.map Scheme.mono ++ Γ) Δ v x .P v' →
      DoOk Γ Δ (.group e₁ mid eₙ :: ss) r (.app m x) φ

theorem All2.length_eq {α β : Type} {R : α → β → Prop} {l : List α} {l' : List β}
    (h : All2 R l l') : l.length = l'.length := by
  induction h with
  | nil => rfl
  | cons _ _ ih => simp [ih]

theorem All2.append {α β : Type} {R : α → β → Prop} {l₁ l₂ : List α} {l₁' l₂' : List β}
    (h₁ : All2 R l₁ l₁') (h₂ : All2 R l₂ l₂') : All2 R (l₁ ++ l₂) (l₁' ++ l₂') := by
  induction h₁ with
  | nil => exact h₂
  | cons r _ ih => exact .cons r ih

theorem curry_append (q : Ty) (l : List Ty) (a x : Ty) :
    curry q (l ++ [a]) x = curry q l (.arr .P a x q) := by
  simp [curry, List.foldr_append]

/-- `flat_map k e` -/
theorem elab_bind {Γ Δ k e m a b c φ d k' e'} (hd : Ent P Δ [⟨S.monad, [m]⟩] [d])
    (hk : Elab P Γ Δ k (.arr .P a (.app m b) c) φ k') (he : Elab P Γ Δ e (.app m a) φ e') :
    ∃ e'', Elab P Γ Δ (S.bind k e) (.app m b) φ e'' := by
  have hm := elab_meth (Γ := Γ) (φ := φ) S.hFlatMap rfl (ts := [m, a, b, c]) rfl
    (by simpa [Cn.subst, Ty.subst, inst] using hd)
    (τ := arrP (.arr .P a (.app m b) c) (.arr .P (.app m a) (.app m b) c)) rfl
  exact ⟨_, .app (.app hm hk (.inl rfl)) he (.inl rfl)⟩

/-- **`do` elaborates.** -/
theorem do_elab {Γ Δ ss r τ φ} (h : DoOk S Γ Δ ss r τ φ) :
    ∃ e', Elab P Γ Δ (S.desugarDo ss r) τ φ e' := by
  induction h with
  | ret h => exact ⟨_, h⟩
  | @bind Γ _ _ _ _ _ _ _ _ _ _ hd he _ ih =>
    obtain ⟨_, hk⟩ := ih
    exact elab_bind S hd (.lam (φ := .P) hk (lam_cov (cov_ctxRow Γ))) he
  | @exp Γ _ _ _ _ _ _ _ _ _ _ hd he _ ih =>
    obtain ⟨_, hk⟩ := ih
    exact elab_bind S hd (.lam (φ := .P) (hk.shift _) (lam_cov (cov_ctxRow Γ))) he
  | let_ he _ ih => obtain ⟨_, hb⟩ := ih; exact ⟨_, .let_ he hb⟩
  | letSig he hp hφ hq _ ih => obtain ⟨_, hb⟩ := ih; exact ⟨_, .letSig he hp hφ hq hb⟩
  | @guard Γ Δ c ss r m b φ d c' hd hc _ ih =>
    obtain ⟨_, hb⟩ := ih
    have hn := elab_meth (Γ := Γ) (φ := φ) S.hNothing rfl (ts := [m, b]) rfl
      (by simpa [Cn.subst, Ty.subst, inst] using hd) (τ := .app m b) rfl
    exact ⟨_, .ite hc hb hn⟩
  | @group Γ Δ e₁ mid eₙ ss r m a₁ as aₙ b φ d d' e₁' eₙ' hp hd hd' h₁ hs hₙ _ ih =>
    obtain ⟨_, hk⟩ := ih
    have hl := hs.length_eq
    obtain ⟨_, hK⟩ := elab_lams (as := a₁ :: as ++ [aₙ]) (ψ := φ) (List.cons_ne_nil _ _)
      (cov_ctxRow _) hk
    have hu := elab_unc S (Γ := Γ) (Δ := Δ) (φ := φ) a₁ (as ++ [aₙ]) (.app m b)
      (ctxRow ((a₁ :: as ++ [aₙ]).reverse.map Scheme.mono ++ Γ))
    obtain ⟨_, ht⟩ := elab_tuple S hd' (hs.append (.cons ⟨_, hₙ⟩ .nil)) h₁
    obtain ⟨_, he⟩ := elab_bind S hd (.app hu hK (.inl rfl)) ht
    simp only [Sugar.desugarDo, hp]
    simp only [List.length_append, List.length_cons, List.length_nil] at he hK
    rw [hl]; exact ⟨_, he⟩
  | @groupPure Γ Δ e₁ mid eₙ ss r v m a₁ as aₙ x φ d' e₁' eₙ' v' hp hd' h₁ hs hₙ hv =>
    have hl := hs.length_eq
    let q := ctxRow ((a₁ :: as ++ [aₙ]).reverse.map Scheme.mono ++ Γ)
    obtain ⟨_, hK⟩ := elab_lams (q := q) (as := a₁ :: as ++ [aₙ]) (ψ := φ) (List.cons_ne_nil _ _)
      (cov_ctxRow _) hv
    rw [curry_append] at hK
    have hu := elab_unc S (Γ := Γ) (Δ := Δ) (φ := φ) a₁ as (.arr .P aₙ x q) q
    obtain ⟨_, ht⟩ := elab_tuple S hd' hs h₁
    have hm := elab_meth (Γ := Γ) (φ := φ) S.hMap2 rfl (ts := [m, nest S.tup a₁ as, aₙ, x, q]) rfl
      (by simpa [Cn.subst, Ty.subst, inst] using hd')
      (τ := arrP (.arr .P (nest S.tup a₁ as) (.arr .P aₙ x q) q)
        (.arr .P (.app m (nest S.tup a₁ as)) (.arr .P (.app m aₙ) (.app m x) q) q)) rfl
    simp only [Sugar.desugarDo, hp]
    simp only [List.length_append, List.length_cons, List.length_nil] at hK
    rw [hl]
    exact ⟨_, .app (.app (.app hm (.app hu hK (.inl rfl)) (.inl rfl)) ht (.inl rfl)) hₙ (.inl rfl)⟩

/-- **T6(a) for `do`.** -/
theorem T6a_do (X : Target P) {Γ Δ ss r τ φ} (h : DoOk S Γ Δ ss r τ φ) :
    ∃ e', Elab P Γ Δ (S.desugarDo ss r) τ φ e' ∧ X.Has (Γ.map Scheme.tr) Δ e' (.mono τ) φ :=
  let ⟨_, he⟩ := do_elab S h
  ⟨_, he, T6a X he⟩

end Flick.Elab
