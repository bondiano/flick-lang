import Formal.FunctionTyping

/-!
# Handler witnesses through type conversion

Runtime forms retain their store cells, regions and parked terms. Conversion
composes with the result comparison and leaves their stage obligations intact.
-/

namespace Flick

variable {S : Sig}

theorem Typ.hwith_type {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ p ℓ E al rf body T φ, e = .hwith p ℓ E al rf body → J = .ex T φ →
      ∃ t' r ε t ψ C φ₀,
        Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) ∧
        Typ S Ψ Θ Γ al (.harms E t' (.rt r ε) (.rg p)) ∧
        Typ S Ψ Θ Γ rf (.ex (.arrow t ψ t' C) (.rt r ε)) ∧ (Stage.use ψ C).le (.rt r ε) ∧
        Typ S Ψ Θ Γ body (.ex t φ₀) ∧ φ₀.le (.rt (Row.hP Θ p E r) ε) ∧
        S.Handles E al r ∧ (Stage.rt r ε).le φ ∧ LeLive S t' T ∧ φ.svc (.atm (.r p)) := by
  induction h with
  | conv _ he ih =>
    intro p ℓ E al rf body T φ hv hJ; cases hJ
    obtain ⟨t', r, ε, t, ψ, C, φ₀, hℓ, hal, hrf, hψ, hb, hφ₀, hc, hφ, ht, hp⟩ :=
      ih p ℓ E al rf body _ _ hv rfl
    exact ⟨t', r, ε, t, ψ, C, φ₀, hℓ, hal, hrf, hψ, hb, hφ₀, hc, hφ, ht.trans (.equiv he), hp⟩
  | hwith hℓ hal hrf hψ hb hφ₀ hc hφ ht hp =>
    intro p ℓ E al rf body T φ hv hJ; cases hv; cases hJ
    exact ⟨_, _, _, _, _, _, _, hℓ, hal, hrf, hψ, hb, hφ₀, hc, hφ, ht, hp⟩
  | allI hv | errVal hv => intro p ℓ E al rf body T φ he _; cases he; cases hv
  | _ => intro p ℓ E al rf body T φ hv _; cases hv

theorem Typ.unw_type {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ p ℓ a T φ, e = .unw p ℓ a → J = .ex T φ →
      ∃ t r ε, Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t Caps.none) ∧
        Typ S Ψ Θ Γ a (.ex t (Stage.use (.rt r ε) (Ty.rg p).reach)) ∧
        (Stage.rt Row.none Row.none).le φ := by
  induction h with
  | conv _ _ ih => intro p ℓ a T φ hv hJ; cases hJ; exact ih p ℓ a _ _ hv rfl
  | unw hl ha hφ => intro p ℓ a T φ hv hJ; cases hv; cases hJ; exact ⟨_, _, _, hl, ha, hφ⟩
  | allI hv | errVal hv => intro p ℓ a T φ he _; cases he; cases hv
  | _ => intro p ℓ a T φ hv _; cases hv

theorem Typ.hctl_type {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ p ℓ κ tl E al rf a k T φ, e = .hctl p ℓ κ tl E al rf a k → J = .ex T φ →
      ∃ t' r ε t ψ C o c a₀ b₀ φ₀ Ei,
        Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) ∧
        Typ S Ψ Θ Γ al (.harms E t' (.rt r ε) (.rg p)) ∧
        Typ S Ψ Θ Γ rf (.ex (.arrow t ψ t' C) (.rt r ε)) ∧ (Stage.use ψ C).le (.rt r ε) ∧
        S.op E o = some (c, a₀, b₀) ∧ c ≠ .abort ∧ (tl = true ↔ c = .tail) ∧
        OkE Ei ∧ k = plug Ei (.hole p) ∧
        Typ S Ψ Θ [] (plug Ei (.app (.const (.perform E o c)) .todo)) (.ex t φ₀) ∧
        φ₀.le (.rt (Row.hP Θ p E r) ε) ∧
        Typ S Ψ Θ Γ a (.ex (if tl = true then b₀ else t')
          (Stage.use (.rt r ε) (if tl = true then Caps.none else (Ty.rg p).reach))) ∧
        (tl = false → ∃ C', Ψ[κ]? = some (.arrow b₀ (.rt r ε) t' C') ∧ C' (.r p)) ∧
        S.Handles E al r ∧ (Stage.rt r ε).le φ ∧ LeLive S t' T ∧ φ.svc (.atm (.r p)) := by
  induction h with
  | conv _ he ih =>
    intro p ℓ κ tl E al rf a k T φ hv hJ; cases hJ
    obtain ⟨t', r, ε, t, ψ, C, o, c, a₀, b₀, φ₀, Ei,
      hl, hal, hrf, hψ, hop, hc, htl, hE, hk, hp, hφ₀, ha, hκ, hcm, hφ, ht, hsvc⟩ :=
      ih p ℓ κ tl E al rf a k _ _ hv rfl
    exact ⟨t', r, ε, t, ψ, C, o, c, a₀, b₀, φ₀, Ei,
      hl, hal, hrf, hψ, hop, hc, htl, hE, hk, hp, hφ₀, ha, hκ, hcm, hφ, ht.trans (.equiv he), hsvc⟩
  | hctl hl hal hrf hψ hop hc htl hE hk hp hφ₀ ha hκ hcm hφ ht hsvc =>
    intro p ℓ κ tl E al rf a k T φ hv hJ; cases hv; cases hJ
    exact ⟨_, _, _, _, _, _, _, _, _, _, _, _,
      hl, hal, hrf, hψ, hop, hc, htl, hE, hk, hp, hφ₀, ha, hκ, hcm, hφ, ht, hsvc⟩
  | allI hv | errVal hv => intro p ℓ κ tl E al rf a k T φ he _; cases he; cases hv
  | _ => intro p ℓ κ tl E al rf a k T φ hv _; cases hv

theorem Typ.hkeep_type {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ q ℓ E al body T φ, e = .hkeep q ℓ E al body → J = .ex T φ →
      ∃ t' r ε t φ₀ r₁ ε₁,
        Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) ∧
        Typ S Ψ Θ Γ al (.harms E t' (.rt r ε) (.rg q)) ∧
        Typ S Ψ Θ Γ body (.ex t φ₀) ∧ φ₀.le (.rt (Row.hP Θ q E r₁) ε₁) ∧
        S.Handles E al r₁ ∧ S.NonTail E r₁ ∧ (Stage.rt r ε).le (.rt r₁ ε₁) ∧
        (Stage.rt r₁ ε₁).le φ ∧ LeLive S t T := by
  induction h with
  | conv _ he ih =>
    intro q ℓ E al body T φ hv hJ; cases hJ
    obtain ⟨t', r, ε, t, φ₀, r₁, ε₁, hl, hal, hb, hφ₀, hc, hn, hle, hφ, ht⟩ :=
      ih q ℓ E al body _ _ hv rfl
    exact ⟨t', r, ε, t, φ₀, r₁, ε₁, hl, hal, hb, hφ₀, hc, hn, hle, hφ, ht.trans (.equiv he)⟩
  | hkeep hl hal hb hφ₀ hc hn hle hφ ht =>
    intro q ℓ E al body T φ hv hJ; cases hv; cases hJ
    exact ⟨_, _, _, _, _, _, _, hl, hal, hb, hφ₀, hc, hn, hle, hφ, ht⟩
  | allI hv | errVal hv => intro q ℓ E al body T φ he _; cases he; cases hv
  | _ => intro q ℓ E al body T φ hv _; cases hv

theorem Typ.hkarm_type {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ q ℓ E al a k T φ, e = .hkarm q ℓ E al a k → J = .ex T φ →
      ∃ t' r ε o a₀ b₀ t φ₀ r₁ ε₁ Ei,
        Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) ∧
        Typ S Ψ Θ Γ al (.harms E t' (.rt r ε) (.rg q)) ∧
        S.op E o = some (.tail, a₀, b₀) ∧ OkE Ei ∧ k = plug Ei (.hole q) ∧
        Typ S Ψ Θ [] (plug Ei (.app (.const (.perform E o .tail)) .todo)) (.ex t φ₀) ∧
        φ₀.le (.rt (Row.hP Θ q E r₁) ε₁) ∧ S.Handles E al r₁ ∧ S.NonTail E r₁ ∧
        Typ S Ψ Θ Γ a (.ex b₀ (.rt r ε)) ∧ (Stage.rt r ε).le (.rt r₁ ε₁) ∧
        (Stage.rt r₁ ε₁).le φ ∧ LeLive S t T := by
  induction h with
  | conv _ he ih =>
    intro q ℓ E al a k T φ hv hJ; cases hJ
    obtain ⟨t', r, ε, o, a₀, b₀, t, φ₀, r₁, ε₁, Ei,
      hl, hal, hop, hE, hk, hp, hφ₀, hc, hn, ha, hle, hφ, ht⟩ := ih q ℓ E al a k _ _ hv rfl
    exact ⟨t', r, ε, o, a₀, b₀, t, φ₀, r₁, ε₁, Ei,
      hl, hal, hop, hE, hk, hp, hφ₀, hc, hn, ha, hle, hφ, ht.trans (.equiv he)⟩
  | hkarm hl hal hop hE hk hp hφ₀ hc hn ha hle hφ ht =>
    intro q ℓ E al a k T φ hv hJ; cases hv; cases hJ
    exact ⟨_, _, _, _, _, _, _, _, _, _, _, hl, hal, hop, hE, hk, hp, hφ₀, hc, hn, ha, hle, hφ, ht⟩
  | allI hv | errVal hv => intro q ℓ E al a k T φ he _; cases he; cases hv
  | _ => intro q ℓ E al a k T φ hv _; cases hv

theorem Typ.hres_type {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ p ℓ a b T φ, e = .hres p ℓ a b → J = .ex T φ →
      ∃ t' r ε κ b₀ C' A,
        Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) ∧
        Typ S Ψ Θ Γ b (.ex t' (Stage.use (.rt r ε) (Ty.rg p).reach)) ∧
        OkE A ∧ a = plug A (.hole p) ∧
        Typ S Ψ Θ [] (plug A (.app (.kont p κ) .todo))
          (.ex t' (Stage.use (.rt r ε) (Ty.rg p).reach)) ∧
        Ψ[κ]? = some (.arrow b₀ (.rt r ε) t' C') ∧ C' (.r p) ∧
        (Stage.rt r ε).le φ ∧ LeLive S t' T := by
  induction h with
  | conv _ he ih =>
    intro p ℓ a b T φ hv hJ; cases hJ
    obtain ⟨t', r, ε, κ, b₀, C', A, hl, hb, hA, ha, hp, hκ, hcp, hφ, ht⟩ :=
      ih p ℓ a b _ _ hv rfl
    exact ⟨t', r, ε, κ, b₀, C', A, hl, hb, hA, ha, hp, hκ, hcp, hφ, ht.trans (.equiv he)⟩
  | hres hl hb hA ha hp hκ hcp hφ ht =>
    intro p ℓ a b T φ hv hJ; cases hv; cases hJ
    exact ⟨_, _, _, _, _, _, _, hl, hb, hA, ha, hp, hκ, hcp, hφ, ht⟩
  | allI hv | errVal hv => intro p ℓ a b T φ he _; cases he; cases hv
  | _ => intro p ℓ a b T φ hv _; cases hv

theorem Typ.inside_type {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ p body T φ, e = .inside p body → J = .ex T φ →
      ∃ F t φ₀ r ε,
        (∀ C s, F C s → s = .rg p ∧ ∀ q, C (.r q) → q < p) ∧
        Typ S Ψ (Facts.union Θ F) Γ body (.ex t φ₀) ∧
        φ₀.le (.rt (Row.union (Row.scope (.rg p)) (Row.union r (Row.disch (.r p) r))) ε) ∧
        ¬ t.reach (.r p) ∧ ¬ (Ty.errors ε).reach (.r p) ∧
        (Stage.rt r ε).le φ ∧ LeLive S t T := by
  induction h with
  | conv _ he ih =>
    intro p body T φ hv hJ; cases hJ
    obtain ⟨F, t, φ₀, r, ε, hF, hb, hφ₀, ht, hε, hφ, hT⟩ := ih p body _ _ hv rfl
    exact ⟨F, t, φ₀, r, ε, hF, hb, hφ₀, ht, hε, hφ, hT.trans (.equiv he)⟩
  | inside hF hb hφ₀ ht hε hφ hT =>
    intro p body T φ hv hJ; cases hv; cases hJ
    exact ⟨_, _, _, _, _, hF, hb, hφ₀, ht, hε, hφ, hT⟩
  | allI hv | errVal hv => intro p body T φ he _; cases he; cases hv
  | _ => intro p body T φ hv _; cases hv

/-- Raising chooses no value result type. -/
theorem Typ.raise_retype {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ w T φ U, e = .raise w → J = .ex T φ → Typ S Ψ Θ Γ (.raise w) (.ex U φ) := by
  induction h with
  | conv _ _ ih => intro w T φ U hv hJ; cases hJ; exact ih w _ _ U hv rfl
  | raise hw hφ => intro w T φ U hv hJ; cases hv; cases hJ; exact .raise hw hφ
  | raiseAdt hw hU hφ => intro w T φ U hv hJ; cases hv; cases hJ; exact .raiseAdt hw hU hφ
  | allI hv | errVal hv => intro w T φ U he _; cases he; cases hv
  | _ => intro w T φ U hv _; cases hv

end Flick
