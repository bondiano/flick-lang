import Formal.SpineTyping

/-!
# Function witnesses through type conversion

Inverting a converted function preserves its implementation and composes the
result comparison. Declared operations use their signature, ordinary constants
their checked instance, and continuations their store cell.
-/

namespace Flick

variable {S : Sig}

theorem Typ.app_arrow {Ψ Θ Γ f x a ψ t C φ}
    (h : Typ S Ψ Θ Γ (.app f x) (.ex (.arrow a ψ t C) φ)) :
    ∃ A χ u D, Typ S Ψ Θ Γ f (.ex (.arrow A χ u D) φ) ∧
      Typ S Ψ Θ Γ x (.ex A φ) ∧ (Stage.use χ D).le φ ∧ LeLive S u (.arrow a ψ t C) :=
  h.app_type f x _ φ rfl rfl (by simp [Ty.isAll]) (by simp [Ty.kind])

private theorem Typ.lamCore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ b T φ, e = .lam b → J = .ex T φ → ¬ T.isAll → T.kind ≠ 7 →
      ∃ A χ u D, Typ S Ψ Θ (.mono A :: Γ) b (.ex u (Stage.use χ D)) ∧
        b.NoIns ∧ LeLive S (.arrow A χ u D) T := by
  induction h with
  | conv _ he ih =>
    intro b T φ hc hJ hA hk; cases hJ
    obtain ⟨A, χ, u, D, hb, hn, ht⟩ := ih b _ _ hc rfl
      (by rw [he.isAll]; exact hA) (by rw [he.kind]; exact hk)
    exact ⟨A, χ, u, D, hb, hn, ht.trans (.equiv he)⟩
  | lam hb hn ht => intro b T φ hc hJ _ _; cases hc; cases hJ; exact ⟨_, _, _, _, hb, hn, ht⟩
  | allI => intro b T φ _ hJ hA _; cases hJ; exact (hA trivial).elim
  | errVal => intro b T φ _ hJ _ hk; cases hJ; exact (hk rfl).elim
  | _ => intro b T φ hc _ _ _; cases hc

theorem Typ.lam_arrow {Ψ Θ Γ b a ψ t C φ}
    (h : Typ S Ψ Θ Γ (.lam b) (.ex (.arrow a ψ t C) φ)) :
    ∃ A χ u D, Typ S Ψ Θ (.mono A :: Γ) b (.ex u (Stage.use χ D)) ∧
      b.NoIns ∧ LeLive S (.arrow A χ u D) (.arrow a ψ t C) :=
  h.lamCore b _ φ rfl rfl (by simp [Ty.isAll]) (by simp [Ty.kind])

private theorem Typ.fixCore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ b T φ, e = .fix b → J = .ex T φ → ¬ T.isAll → T.kind ≠ 7 →
      ∃ A χ u D, Typ S Ψ Θ (.mono A :: .mono (.arrow A χ u D) :: Γ) b (.ex u (Stage.use χ D)) ∧
        b.NoIns ∧ LeLive S (.arrow A χ u D) T := by
  induction h with
  | conv _ he ih =>
    intro b T φ hc hJ hA hk; cases hJ
    obtain ⟨A, χ, u, D, hb, hn, ht⟩ := ih b _ _ hc rfl
      (by rw [he.isAll]; exact hA) (by rw [he.kind]; exact hk)
    exact ⟨A, χ, u, D, hb, hn, ht.trans (.equiv he)⟩
  | fix hb hn ht => intro b T φ hc hJ _ _; cases hc; cases hJ; exact ⟨_, _, _, _, hb, hn, ht⟩
  | allI => intro b T φ _ hJ hA _; cases hJ; exact (hA trivial).elim
  | errVal => intro b T φ _ hJ _ hk; cases hJ; exact (hk rfl).elim
  | _ => intro b T φ hc _ _ _; cases hc

theorem Typ.fix_arrow {Ψ Θ Γ b a ψ t C φ}
    (h : Typ S Ψ Θ Γ (.fix b) (.ex (.arrow a ψ t C) φ)) :
    ∃ A χ u D, Typ S Ψ Θ (.mono A :: .mono (.arrow A χ u D) :: Γ) b (.ex u (Stage.use χ D)) ∧
      b.NoIns ∧ LeLive S (.arrow A χ u D) (.arrow a ψ t C) :=
  h.fixCore b _ φ rfl rfl (by simp [Ty.isAll]) (by simp [Ty.kind])

private theorem Typ.constCore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ c T φ, e = .const c → J = .ex T φ → ¬ T.isAll → T.kind ≠ 7 →
      (∀ E o cl, c ≠ .perform E o cl) →
      ∃ ι, c.ok Θ ι ∧ LeLive S (c.ty S ι) T := by
  induction h with
  | conv _ he ih =>
    intro c T φ hc hJ hA hk hn; cases hJ
    obtain ⟨ι, hi, ht⟩ := ih c _ _ hc rfl (by rw [he.isAll]; exact hA)
      (by rw [he.kind]; exact hk) hn
    exact ⟨ι, hi, ht.trans (.equiv he)⟩
  | const hi ht => intro c T φ hc hJ _ _ _; cases hc; cases hJ; exact ⟨_, hi, ht⟩
  | perform => intro c T φ hc _ _ _ hn; cases hc; exact (hn _ _ _ rfl).elim
  | allI => intro c T φ _ hJ hA _ _; cases hJ; exact (hA trivial).elim
  | errVal => intro c T φ _ hJ _ hk _; cases hJ; exact (hk rfl).elim
  | _ => intro c T φ hc _ _ _ _; cases hc

theorem Typ.const_arrow {Ψ Θ Γ c a ψ t C φ}
    (h : Typ S Ψ Θ Γ (.const c) (.ex (.arrow a ψ t C) φ))
    (hn : ∀ E o cl, c ≠ .perform E o cl) :
    ∃ ι, c.ok Θ ι ∧ LeLive S (c.ty S ι) (.arrow a ψ t C) :=
  h.constCore c _ φ rfl rfl (by simp [Ty.isAll]) (by simp [Ty.kind]) hn

private theorem Typ.performCore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ E o cl T φ, e = .const (.perform E o cl) → J = .ex T φ → ¬ T.isAll → T.kind ≠ 7 →
      ∃ a b u, S.op E o = some (cl, a, b) ∧
        LeLive S (.arrow a (.rt (Row.single E) Row.none) (if cl = .abort then u else b) Caps.none) T := by
  induction h with
  | conv _ he ih =>
    intro E o cl T φ hc hJ hA hk; cases hJ
    obtain ⟨a, b, u, hop, ht⟩ := ih E o cl _ _ hc rfl
      (by rw [he.isAll]; exact hA) (by rw [he.kind]; exact hk)
    exact ⟨a, b, u, hop, ht.trans (.equiv he)⟩
  | perform hop ht => intro E o cl T φ hc hJ _ _; cases hc; cases hJ; exact ⟨_, _, _, hop, ht⟩
  | const hi _ => intro E o cl T φ hc _ _ _; cases hc; exact hi.elim
  | allI => intro E o cl T φ _ hJ hA _; cases hJ; exact (hA trivial).elim
  | errVal => intro E o cl T φ _ hJ _ hk; cases hJ; exact (hk rfl).elim
  | _ => intro E o cl T φ hc _ _ _; cases hc

theorem Typ.perform_arrow {Ψ Θ Γ E o cl a ψ t C φ}
    (h : Typ S Ψ Θ Γ (.const (.perform E o cl)) (.ex (.arrow a ψ t C) φ)) :
    ∃ a₀ b₀ u, S.op E o = some (cl, a₀, b₀) ∧
      LeLive S (.arrow a₀ (.rt (Row.single E) Row.none) (if cl = .abort then u else b₀) Caps.none)
        (.arrow a ψ t C) :=
  h.performCore E o cl _ φ rfl rfl (by simp [Ty.isAll]) (by simp [Ty.kind])

/-- Every function constant has an instance of its signature below the requested arrow. -/
theorem Typ.const_witness {Ψ Θ Γ c a ψ t C φ}
    (h : Typ S Ψ Θ Γ (.const c) (.ex (.arrow a ψ t C) φ)) :
    ∃ ι, LeLive S (c.ty S ι) (.arrow a ψ t C) := by
  cases c with
  | perform E o cl =>
    obtain ⟨_, _, u, hop, ht⟩ := h.perform_arrow
    refine ⟨{ (default : CArg) with t := u }, ?_⟩
    cases cl <;> simpa [Const.ty, hop] using ht
  | _ =>
    obtain ⟨ι, _, ht⟩ := h.const_arrow (by intro E o cl h; cases h)
    exact ⟨ι, ht⟩

private theorem Typ.kontCore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ p ℓ T φ, e = .kont p ℓ → J = .ex T φ → ¬ T.isAll → T.kind ≠ 7 →
      ∃ b ψ t C, Ψ[ℓ]? = some (.arrow b ψ t C) ∧
        C (.r p) ∧ LeLive S (.arrow b ψ t C) T := by
  induction h with
  | conv _ he ih =>
    intro p ℓ T φ hc hJ hA hk; cases hJ
    obtain ⟨b, ψ, t, C, hl, hp, ht⟩ := ih p ℓ _ _ hc rfl
      (by rw [he.isAll]; exact hA) (by rw [he.kind]; exact hk)
    exact ⟨b, ψ, t, C, hl, hp, ht.trans (.equiv he)⟩
  | kont hl hp ht => intro p ℓ T φ hc hJ _ _; cases hc; cases hJ; exact ⟨_, _, _, _, hl, hp, ht⟩
  | allI => intro p ℓ T φ _ hJ hA _; cases hJ; exact (hA trivial).elim
  | errVal => intro p ℓ T φ _ hJ _ hk; cases hJ; exact (hk rfl).elim
  | _ => intro p ℓ T φ hc _ _ _; cases hc

theorem Typ.kont_arrow {Ψ Θ Γ p ℓ a ψ t C φ}
    (h : Typ S Ψ Θ Γ (.kont p ℓ) (.ex (.arrow a ψ t C) φ)) :
    ∃ b ψ' t' C', Ψ[ℓ]? = some (.arrow b ψ' t' C') ∧
      C' (.r p) ∧ LeLive S (.arrow b ψ' t' C') (.arrow a ψ t C) :=
  h.kontCore p ℓ _ φ rfl rfl (by simp [Ty.isAll]) (by simp [Ty.kind])

end Flick
