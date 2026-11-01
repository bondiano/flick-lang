import Formal.Substitution

/-!
# Values are pure (Л1, last clause)

Every value has a type below the given one that it has with `P`; for a
constructor spine that type has only pure arrows.
-/

namespace Flick

variable {S : Sig}

/-- All arrows along the result spine are pure. -/
def Ty.PureSpine : Ty → Prop
  | .arrow _ φ t => φ = .pure ∧ t.PureSpine
  | _ => True

theorem Ty.arrows_pureSpine (ts : List Ty) (T : Name) : (Ty.arrows ts (.adt T)).PureSpine := by
  induction ts with
  | nil => trivial
  | cons a ts ih => exact ⟨rfl, ih⟩

theorem Const.ty_first {c : Const} (h : c.arity = 2 ∨ c.layer) :
    ∃ x y, c.ty S = .arrow x .pure y := by
  cases c <;> simp [Const.arity, Const.layer] at h <;> exact ⟨_, _, rfl⟩

theorem Const.ty_merge {c : Const} (h₁ : c.arity = 2) (h₂ : c.layer) :
    ∃ x y z, c.ty S = .arrow x .pure (.arrow y .pure z) := by
  cases c <;> simp [Const.arity, Const.layer] at h₁ h₂
  exact ⟨_, _, _, rfl⟩

theorem Const.ty_arrow (c : Const) : ∃ x φ y, c.ty S = .arrow x φ y := by
  cases c <;> exact ⟨_, _, _, rfl⟩

/-- What `Value.pure` gives. -/
def PureTy (S : Sig) (Ψ Γ : List Ty) (e : Tm) (T : Ty) : Prop :=
  ∃ T₀, Le T₀ T ∧ Typ S Ψ Γ e (.ex T₀ .pure) ∧
    (∀ K, e.head = .ctor K → T₀.PureSpine) ∧
    (∀ c, e = .const c → T₀ = c.ty S) ∧
    (∀ c e₁, e = .app (.const c) e₁ → c.arity = 2 → c.layer → ∃ y z, T₀ = .arrow y .pure z)

theorem Typ.pureTy {Ψ Γ e J} (h : Typ S Ψ Γ e J) :
    Value e → ∀ {T φ}, J = .ex T φ → PureTy S Ψ Γ e T := by
  induction h with
  | lam hb hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨_, hT, .lam hb .refl, by simp [Tm.head], by simp, by simp⟩
  | fix hf hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨_, hT, .fix hf .refl, by simp [Tm.head], by simp, by simp⟩
  | lit => intro _ _ _ hJ; cases hJ; exact ⟨_, .refl, .lit, by simp [Tm.head], by simp, by simp⟩
  | loc hl =>
    intro _ _ _ hJ; cases hJ; exact ⟨_, .refl, .loc hl, by simp [Tm.head], by simp, by simp⟩
  | const hc hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨_, hT, .const hc .refl, by simp [Tm.head], by simp, by simp⟩
  | ctor hK hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨_, hT, .ctor hK .refl, fun _ _ => Ty.arrows_pureSpine _ _, by simp, by simp⟩
  | rnil => intro _ _ _ hJ; cases hJ; exact ⟨_, .refl, .rnil, by simp [Tm.head], by simp, by simp⟩
  | enil hρ =>
    intro _ _ _ hJ; cases hJ
    exact ⟨_, .refl, .enil hρ, by simp [Tm.head], by simp, by simp⟩
  | task he hψ hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨_, hT, .task he hψ .refl, by simp [Tm.head], by simp, by simp⟩
  | rcons hl _ _ ih₁ ih₂ =>
    intro hv _ _ hJ; cases hJ
    cases hv with
    | rcons hv₁ hv₂ =>
      obtain ⟨_, l₁, t₁, -⟩ := ih₁ hv₁ rfl
      obtain ⟨_, l₂, t₂, -⟩ := ih₂ hv₂ rfl
      exact ⟨_, .refl, .rcons hl (t₁.sub ⟨l₁, Stage.le_refl _⟩) (t₂.sub ⟨l₂, Stage.le_refl _⟩),
        by simp [Tm.head], by simp, by simp⟩
  | econs hv₁ hθ _ _ hρ ih₁ ih₂ =>
    intro hv _ _ hJ; cases hJ
    cases hv with
    | econs _ hv₂ =>
      obtain ⟨_, l₁, t₁, -⟩ := ih₁ hv₁ rfl
      obtain ⟨_, l₂, t₂, -⟩ := ih₂ hv₂ rfl
      exact ⟨_, .refl, .econs hv₁ hθ (t₁.sub ⟨l₁, Stage.le_refl _⟩) (t₂.sub ⟨l₂, Stage.le_refl _⟩) hρ,
        by simp [Tm.head], by simp, by simp⟩
  | errVal hv' _ hE ih =>
    intro _ _ _ hJ; cases hJ
    obtain ⟨T₀, l, t, _, hc, ha⟩ := ih hv' rfl
    cases l
    refine ⟨_, .refl, .errVal hv' t hE, fun _ _ => trivial, fun c hc' => ?_, fun c _ he h₁ h₂ => ?_⟩
    · obtain ⟨_, _, _, h⟩ := Const.ty_arrow (S := S) c
      have := hc c hc'; rw [h] at this; cases this
    · obtain ⟨_, _, h⟩ := ha c _ he h₁ h₂; cases h
  | app hf he hψ hT ih₁ ih₂ =>
    intro hv _ _ hJ; cases hJ
    cases hv with
    | ctorApp hvf hK hve =>
      obtain ⟨T₀, l₀, t₀, sp, -⟩ := ih₁ hvf rfl
      obtain ⟨_, _, _, rfl, la, lφ, lt⟩ := l₀.arrow_right
      obtain ⟨rfl, sp'⟩ := sp _ hK
      obtain ⟨_, le, te, -⟩ := ih₂ hve rfl
      refine ⟨_, lt.trans hT, .app t₀ (te.sub ⟨le.trans la, Stage.le_refl _⟩) trivial .refl,
        fun _ _ => sp', fun _ h => (by cases h), fun c _ h => ?_⟩
      cases h; simp [Tm.head] at hK
    | constApp hc hve =>
      obtain ⟨T₀, l₀, t₀, _, hcT, -⟩ := ih₁ .const rfl
      obtain ⟨x, y, hxy⟩ := Const.ty_first (S := S) hc
      rw [hcT _ rfl, hxy] at l₀ t₀
      obtain ⟨_, _, _, h, la, _, lt⟩ := l₀.arrow_left
      cases h
      obtain ⟨_, le, te, -⟩ := ih₂ hve rfl
      refine ⟨_, lt.trans hT, .app t₀ (te.sub ⟨le.trans la, Stage.le_refl _⟩) trivial .refl,
        by simp [Tm.head], fun _ h => (by cases h), fun c' _ h h₁ h₂ => ?_⟩
      cases h
      obtain ⟨_, _, _, h'⟩ := Const.ty_merge (S := S) h₁ h₂
      rw [hxy] at h'; cases h'; exact ⟨_, _, rfl⟩
    | constApp2 h₁ h₂ hv₁ hve =>
      obtain ⟨T₀, l₀, t₀, _, _, ha⟩ := ih₁ (.constApp (.inl h₁) hv₁) rfl
      obtain ⟨y, z, rfl⟩ := ha _ _ rfl h₁ h₂
      obtain ⟨_, _, _, h, la, _, lt⟩ := l₀.arrow_left
      cases h
      obtain ⟨_, le, te, -⟩ := ih₂ hve rfl
      exact ⟨_, lt.trans hT, .app t₀ (te.sub ⟨le.trans la, Stage.le_refl _⟩) trivial .refl,
        by simp [Tm.head], fun _ h => (by cases h), fun _ _ h => (by cases h)⟩
  | _ => intro hv _ _ hJ; cases hJ <;> cases hv

/-- Л1, last clause: a value has its type with any stage. -/
theorem Typ.valueAny {Ψ Γ v T φ} (h : Typ S Ψ Γ v (.ex T φ)) (hv : Value v) (ψ : Stage) :
    Typ S Ψ Γ v (.ex T ψ) := by
  obtain ⟨_, l, t, -⟩ := h.pureTy hv rfl
  exact t.sub ⟨l, Stage.pure_le _⟩

end Flick
