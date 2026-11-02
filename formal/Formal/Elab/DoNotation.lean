import Formal.Elab.SugarBase

/-!
# Elaboration: `do` (elaboration.md §3.1)

`DoOk` is the checker's typing of a `do` block (`do_notation.rs`, `applicative_do.rs`): the
rest of a block after `<-` is the pure continuation of `flat_map`, a guard needs
`Alternative m`, a group `and` needs `Applicative m` and ends either in `pure e` (`map2`
alone) or in the rest (`flat_map` over the tuple). `do_elab`: the desugared block elaborates
at the container and element type of the block, with the row its desugaring shows (`docs/formal/elaboration.md#value-qualifiers-in-prelude-schemes`);
`T6a_do`: T6(a) covers it.
-/

namespace Flick.Elab

variable {P : Prog} (S : Sugar P)

/-- The typing of `do { s̄; r }` at the stage `φ`, a block of the container `m` with elements
of type `b`. The rows of the values are not indices: the core has no (≤-Cap) on them (`docs/formal/elaboration.md#value-qualifiers-in-prelude-schemes`),
so a block has the row its desugaring shows, and each premise takes any row. -/
inductive DoOk : List Scheme → List Cn → List Stmt → Exp → Ty → Ty → Stage → Prop
  | ret {Γ Δ r m q b φ r'} : Elab P Γ Δ r (appQ m q b) φ r' → DoOk Γ Δ [] r m b φ
  | bind {Γ Δ e ss r m d a b φ dk e'} : Ent P Δ [⟨S.monad, [m]⟩] [dk] →
      Elab P Γ Δ e (appQ m d a) φ e' → DoOk (Scheme.mono a :: Γ) Δ ss r m b .P →
      DoOk Γ Δ (.bind e :: ss) r m b φ
  | exp {Γ Δ e ss r m d a b φ dk e'} : Ent P Δ [⟨S.monad, [m]⟩] [dk] →
      Elab P Γ Δ e (appQ m d a) φ e' → DoOk Γ Δ ss r m b .P →
      DoOk Γ Δ (.exp e :: ss) r m b φ
  | let_ {Γ Δ e ss r a m b φ e'} : Elab P Γ Δ e a φ e' → DoOk (Scheme.mono a :: Γ) Δ ss r m b φ →
      DoOk Γ Δ (.let_ e :: ss) r m b φ
  | letSig {Γ Δ σ e ss r m b φ₁ φ e'} :
      Elab P (Γ.map (·.subst (up σ.k))) (σ.ctx ++ Δ.map (·.subst (up σ.k))) e σ.ty φ₁ e' →
      (σ.ctx ≠ [] ∨ σ.k ≠ 0 → φ₁ = .P) → (φ₁ = .P ∨ φ₁ = φ) →
      (σ.ctx ≠ [] → ∀ n, heldS (Γ.map (·.subst (up σ.k))) e n → σ.ty.reach n ∨ σ.q.reach n) →
      DoOk (σ :: Γ) Δ ss r m b φ →
      DoOk Γ Δ (.letSig σ e :: ss) r m b φ
  | guard {Γ Δ c ss r m b φ dk c'} : Ent P Δ [⟨S.alternative, [m]⟩] [dk] →
      Elab P Γ Δ c boolTy φ c' → DoOk Γ Δ ss r m b φ →
      DoOk Γ Δ (.guard c :: ss) r m b φ
  | group {Γ Δ e₁ mid eₙ ss r m d₁ a₁ as dₙ aₙ b φ dk dk' e₁' eₙ'} : S.pureLast ss r = none →
      Ent P Δ [⟨S.monad, [m]⟩] [dk] → Ent P Δ [⟨S.applicative, [m]⟩] [dk'] →
      Elab P Γ Δ e₁ (appQ m d₁ a₁) φ e₁' →
      All2 (fun e a => ∃ d e', Elab P Γ Δ e (appQ m d a) φ e') mid as →
      Elab P Γ Δ eₙ (appQ m dₙ aₙ) φ eₙ' →
      DoOk ((a₁ :: as ++ [aₙ]).reverse.map Scheme.mono ++ Γ) Δ ss r m b .P →
      DoOk Γ Δ (.group e₁ mid eₙ :: ss) r m b φ
  | groupPure {Γ Δ e₁ mid eₙ ss r v m d₁ a₁ as dₙ aₙ x φ dk' e₁' eₙ' v'} :
      S.pureLast ss r = some v →
      Ent P Δ [⟨S.applicative, [m]⟩] [dk'] →
      Elab P Γ Δ e₁ (appQ m d₁ a₁) φ e₁' →
      All2 (fun e a => ∃ d e', Elab P Γ Δ e (appQ m d a) φ e') mid as →
      Elab P Γ Δ eₙ (appQ m dₙ aₙ) φ eₙ' →
      Elab P ((a₁ :: as ++ [aₙ]).reverse.map Scheme.mono ++ Γ) Δ v x .P v' →
      DoOk Γ Δ (.group e₁ mid eₙ :: ss) r m x φ

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
theorem elab_bind {Γ Δ k e m a b c d q φ dk k' e'} (hd : Ent P Δ [⟨S.monad, [m]⟩] [dk])
    (hk : Elab P Γ Δ k (.arr .P a (appQ m q b) c) φ k') (he : Elab P Γ Δ e (appQ m d a) φ e') :
    ∃ q' e'', Elab P Γ Δ (S.bind k e) (appQ m q' b) φ e'' := by
  have hm := elab_meth (Γ := Γ) (φ := φ) S.hFlatMap rfl (ts := [m, c, d, a, b, q]) rfl
    (by simpa [Cn.subst, Ty.subst, inst] using hd) (τ := bindTy m c d a b q) rfl
  exact ⟨_, _, .app (.app hm hk (.inl rfl)) he (.inl rfl)⟩

/-- **`do` elaborates**, at the row its desugaring shows. -/
theorem do_elab {Γ Δ ss r m b φ} (h : DoOk S Γ Δ ss r m b φ) :
    ∃ q e', Elab P Γ Δ (S.desugarDo ss r) (appQ m q b) φ e' := by
  induction h with
  | ret h => exact ⟨_, _, h⟩
  | @bind Γ _ _ _ _ _ _ _ _ _ _ _ hd he _ ih =>
    obtain ⟨_, _, hk⟩ := ih
    exact elab_bind S hd (.lam (φ := .P) hk (lam_cov (cov_ctxRow Γ))) he
  | @exp Γ _ _ _ _ _ _ _ _ _ _ _ hd he _ ih =>
    obtain ⟨_, _, hk⟩ := ih
    exact elab_bind S hd (.lam (φ := .P) (hk.shift _) (lam_cov (cov_ctxRow Γ))) he
  | let_ he _ ih => obtain ⟨_, _, hb⟩ := ih; exact ⟨_, _, .let_ he hb⟩
  | letSig he hp hφ hq _ ih => obtain ⟨_, _, hb⟩ := ih; exact ⟨_, _, .letSig he hp hφ hq hb⟩
  | @guard Γ Δ c ss r m b φ dk c' hd hc _ ih =>
    obtain ⟨q, _, hb⟩ := ih
    have hn := elab_meth (Γ := Γ) (φ := φ) S.hNothing rfl (ts := [m, b, q]) rfl
      (by simpa [Cn.subst, Ty.subst, inst] using hd) (τ := appQ m q b) rfl
    exact ⟨_, _, .ite hc hb hn⟩
  | @group Γ Δ e₁ mid eₙ ss r m d₁ a₁ as dₙ aₙ b φ dk dk' e₁' eₙ' hp hd hd' h₁ hs hₙ _ ih =>
    obtain ⟨qr, _, hk⟩ := ih
    have hl := hs.length_eq
    obtain ⟨_, hK⟩ := elab_lams (as := a₁ :: as ++ [aₙ]) (ψ := φ) (List.cons_ne_nil _ _)
      (cov_ctxRow _) hk
    have hu := elab_unc S (Γ := Γ) (Δ := Δ) (φ := φ) a₁ (as ++ [aₙ]) (appQ m qr b)
      (ctxRow ((a₁ :: as ++ [aₙ]).reverse.map Scheme.mono ++ Γ))
    obtain ⟨_, _, ht⟩ := elab_tuple S hd' (hs.append (.cons ⟨_, _, hₙ⟩ .nil)) h₁
    obtain ⟨_, _, he⟩ := elab_bind S hd (.app hu hK (.inl rfl)) ht
    simp only [Sugar.desugarDo, hp]
    simp only [List.length_append, List.length_cons, List.length_nil] at he hK
    rw [hl]; exact ⟨_, _, he⟩
  | @groupPure Γ Δ e₁ mid eₙ ss r v m d₁ a₁ as dₙ aₙ x φ dk' e₁' eₙ' v' hp hd' h₁ hs hₙ hv =>
    have hl := hs.length_eq
    let q := ctxRow ((a₁ :: as ++ [aₙ]).reverse.map Scheme.mono ++ Γ)
    obtain ⟨_, hK⟩ := elab_lams (q := q) (as := a₁ :: as ++ [aₙ]) (ψ := φ) (List.cons_ne_nil _ _)
      (cov_ctxRow _) hv
    rw [curry_append] at hK
    have hu := elab_unc S (Γ := Γ) (Δ := Δ) (φ := φ) a₁ as (.arr .P aₙ x q) q
    obtain ⟨d, _, ht⟩ := elab_tuple S hd' hs h₁
    have hm := elab_meth (Γ := Γ) (φ := φ) S.hMap2 rfl
      (ts := [m, q, d, nest S.tup a₁ as, aₙ, x, q, dₙ]) rfl
      (by simpa [Cn.subst, Ty.subst, inst] using hd')
      (τ := map2Ty m q d (nest S.tup a₁ as) aₙ x q dₙ) rfl
    simp only [Sugar.desugarDo, hp]
    simp only [List.length_append, List.length_cons, List.length_nil] at hK
    rw [hl]
    exact ⟨_, _, .app (.app (.app hm (.app hu hK (.inl rfl)) (.inl rfl)) ht (.inl rfl)) hₙ (.inl rfl)⟩

/-- **T6(a) for `do`.** -/
theorem T6a_do (X : Target P) {Γ Δ ss r m b φ} (h : DoOk S Γ Δ ss r m b φ) :
    ∃ q e', Elab P Γ Δ (S.desugarDo ss r) (appQ m q b) φ e' ∧
      X.Has (Γ.map Scheme.tr) Δ e' (.mono (appQ m q b)) φ :=
  let ⟨_, _, he⟩ := do_elab S h
  ⟨_, _, he, T6a X he⟩

end Flick.Elab
