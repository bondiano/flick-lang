import Formal.Canonical

/-!
# Kinds of types and shapes of values
-/

namespace Flick

variable {S : Sig}

/-- The outermost type former. -/
def Ty.kind : Ty → Nat
  | .tvar _ => 9
  | .base _ => 0
  | .arrow .. => 1
  | .ref _ => 2
  | .adt _ => 3
  | .rnil | .rcons .. => 4
  | .eff .. => 5
  | .layer .. => 6
  | .errors _ => 7
  | .env _ => 8

theorem Le.kind {a b : Ty} (h : Le a b) : a.kind = b.kind := by
  cases h <;> rfl

theorem Ty.opaque_of_kind {t : Ty} (h : t.kind = 1 ∨ t.kind = 6) : t.Opaque := by
  cases t <;> simp [Ty.kind] at h <;> trivial

theorem Ty.arrows_kind (ts : List Ty) (T : Name) :
    (Ty.arrows ts (.adt T)).kind = 1 ∨ (Ty.arrows ts (.adt T)).kind = 3 := by
  cases ts <;> simp [Ty.arrows, Ty.kind]

/-- The kind of a constant's type after one argument. -/
def Const.resKind : Const → Nat
  | .succeed _ | .effect .. => 6
  | _ => 1

theorem Const.resKind_cases (c : Const) : c.resKind = 1 ∨ c.resKind = 6 := by
  cases c <;> simp [Const.resKind]

theorem Const.ty_kind (c : Const) (τ : Ty) : (c.ty S τ).kind = 1 := by
  cases c <;> rfl

/-- The result of a binary or layer constant after one argument. -/
theorem Const.ty_res {c : Const} {τ : Ty} (h : c.arity = 2 ∨ c.layer) :
    ∃ x y, c.ty S τ = .arrow x .pure y ∧ y.kind = c.resKind := by
  cases c <;> simp [Const.arity, Const.layer] at h <;> exact ⟨_, _, rfl, rfl⟩

theorem Const.res_kind {c : Const} {τ a ψ t T'} (hc : c.arity = 2 ∨ c.layer)
    (h : Le (c.ty S τ) (.arrow a ψ t)) (hT : Le t T') : T'.kind = c.resKind := by
  obtain ⟨x, y, hxy, hk⟩ := Const.ty_res (S := S) (τ := τ) hc
  rw [hxy] at h
  obtain ⟨_, _, _, h', _, _, lt⟩ := h.arrow_left
  cases h'; rw [← hT.kind, ← lt.kind, hk]

theorem Const.res2_kind {c : Const} {τ a ψ t a₂ ψ₂ t₂ T'} (h₁ : c.arity = 2) (h₂ : c.layer)
    (h : Le (c.ty S τ) (.arrow a ψ t)) (h' : Le t (.arrow a₂ ψ₂ t₂)) (hT : Le t₂ T') :
    T'.kind = 6 := by
  cases c <;> simp [Const.arity, Const.layer] at h₁ h₂
  simp only [Const.ty] at h
  obtain ⟨_, _, _, e, _, _, lt⟩ := h.arrow_left
  cases e
  obtain ⟨_, _, _, e, _, _, lt'⟩ := (lt.trans h').arrow_left
  cases e; rw [← hT.kind, ← lt'.kind]; rfl

theorem Typ.constApp_kind {Ψ Γ c w T φ} (h : Typ S Ψ Γ (.app (.const c) w) (.ex T φ))
    (hc : c.arity = 2 ∨ c.layer) : T.kind = c.resKind := by
  cases h with
  | app hf _ _ hT => cases hf with | const _ hT' => exact Const.res_kind hc hT' hT
  | errVal _ h' _ =>
    exfalso
    cases h' with
    | app hf _ _ hT =>
      cases hf with
      | const _ hT' =>
        have := Const.res_kind hc hT' hT
        cases c <;> simp [Const.resKind, Ty.kind] at this

theorem Typ.constApp2_kind {Ψ Γ c w₁ w₂ T φ} (h : Typ S Ψ Γ (.app (.app (.const c) w₁) w₂) (.ex T φ))
    (h₁ : c.arity = 2) (h₂ : c.layer) : T.kind = 6 := by
  have key : ∀ {a ψ t T'}, Typ S Ψ Γ (.app (.const c) w₁) (.ex (.arrow a ψ t) φ) → Le t T' →
      T'.kind = 6 := by
    intro _ _ _ _ hf hT
    cases hf with
    | app hc _ _ hT' => cases hc with | const _ hT'' => exact Const.res2_kind h₁ h₂ hT'' hT' hT
  cases h with
  | app hf _ _ hT => exact key hf hT
  | errVal _ h' _ =>
    exfalso
    cases h' with
    | app hf _ _ hT => have := key hf hT; simp [Ty.kind] at this

theorem Le.opaque {a b : Ty} (h : Le a b) (ha : a.Opaque) : b.Opaque := by
  cases h <;> first | exact ha | trivial

theorem Shp.le {a b : Ty} {e : Tm} (h : Le a b) (hs : Shp S true a e) : Shp S true b e := by
  cases h with
  | refl => exact hs
  | arrow => exact .hidden trivial
  | eff => exact .hidden trivial

theorem Shp.false_arrows {U e} (h : Shp S false U e) :
    ∃ K ts T ts', e.head = .ctor K ∧ S.ctor K = some (ts, T) ∧ U = .arrows ts' (.adt T) := by
  generalize hf : false = fl at h
  induction h with
  | head hK => exact ⟨_, _, _, _, rfl, hK, rfl⟩
  | app _ _ ih _ =>
    obtain ⟨K, ts, T, ts', hh, hK, hU⟩ := ih hf
    cases ts' with
    | nil => cases hU
    | cons a ts' => cases hU; exact ⟨K, ts, T, ts', hh, hK, rfl⟩
  | _ => cases hf

theorem Shp.full_of {U T e} (h : Shp S false U e) (hl : Le U T) : Shp S true T e := by
  obtain ⟨_, _, _, ts', _, _, rfl⟩ := h.false_arrows
  cases ts' with
  | nil => cases hl; exact .adt h
  | cons _ _ => exact .hidden (hl.opaque trivial)

/-- The spine clause of `Typ.shape`. -/
def SpineShp (S : Sig) (Ψ : List Ty) (Γ : List Scheme) (e : Tm) (T : Ty) (φ : Stage) : Prop :=
  (∃ U, Le U T ∧ Shp S false U e) ∨
    ∃ ε E, T = .errors ε ∧ ε E ∧ Shp S false (.adt E) e ∧ Typ S Ψ Γ e (.ex (.adt E) φ)

theorem Typ.shape {Ψ Γ e J} (h : Typ S Ψ Γ e J) : Value e → ∀ {T φ}, J = .ex T φ →
    Shp S true T e ∧ (∀ K, e.head = .ctor K → SpineShp S Ψ Γ e T φ) := by
  induction h with
  | lam _ hT => intro _ _ _ hJ; cases hJ; exact ⟨.hidden (hT.opaque trivial), by simp [Tm.head]⟩
  | fix _ hT => intro _ _ _ hJ; cases hJ; exact ⟨.hidden (hT.opaque trivial), by simp [Tm.head]⟩
  | task _ _ hT => intro _ _ _ hJ; cases hJ; exact ⟨.hidden (hT.opaque trivial), by simp [Tm.head]⟩
  | const _ hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨.hidden (Ty.opaque_of_kind (.inl (by rw [← hT.kind]; exact Const.ty_kind _ _))),
      by simp [Tm.head]⟩
  | loc => intro _ _ _ hJ; cases hJ; exact ⟨.hidden trivial, by simp [Tm.head]⟩
  | lit => intro _ _ _ hJ; cases hJ; exact ⟨.lit, by simp [Tm.head]⟩
  | rnil => intro _ _ _ hJ; cases hJ; exact ⟨.rnil, by simp [Tm.head]⟩
  | enil => intro _ _ _ hJ; cases hJ; exact ⟨.hidden trivial, by simp [Tm.head]⟩
  | econs => intro _ _ _ hJ; cases hJ; exact ⟨.hidden trivial, by simp [Tm.head]⟩
  | rcons _ _ _ _ ih₁ ih₂ =>
    intro hv _ _ hJ; cases hJ
    cases hv with
    | rcons hv₁ hv₂ => exact ⟨.rcons (ih₁ hv₁ rfl).1 (ih₂ hv₂ rfl).1, by simp [Tm.head]⟩
  | ctor hK hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨(Shp.head hK).full_of hT, fun _ _ => .inl ⟨_, hT, .head hK⟩⟩
  | errVal hv' h' hE ih =>
    intro _ _ _ hJ; cases hJ
    refine ⟨.hidden trivial, fun K hK => .inr ⟨_, _, rfl, hE, ?_, h'⟩⟩
    rcases (ih hv' rfl).2 K hK with ⟨U, hU, hs⟩ | ⟨_, _, h, _⟩
    · cases hU; exact hs
    · cases h
  | app hf _ _ hT ih₁ ih₂ =>
    intro hv _ _ hJ; cases hJ
    cases hv with
    | ctorApp hvf hK hve =>
      rcases (ih₁ hvf rfl).2 _ hK with ⟨U, hU, hs⟩ | ⟨_, _, h, _⟩
      · obtain ⟨_, _, _, rfl, la, _, lt⟩ := hU.arrow_right
        obtain ⟨_, _, _, ts', _, _, hU'⟩ := hs.false_arrows
        cases ts' with
        | nil => cases hU'
        | cons _ _ =>
          cases hU'
          have hs' : Shp S false _ _ := .app hs (Shp.le la (ih₂ hve rfl).1)
          exact ⟨hs'.full_of (lt.trans hT), fun _ _ => .inl ⟨_, lt.trans hT, hs'⟩⟩
      · cases h
    | constApp hc _ =>
      have hk := (Typ.app hf ‹_› ‹_› hT).constApp_kind hc
      refine ⟨.hidden (Ty.opaque_of_kind ?_), by simp [Tm.head]⟩
      rw [hk]; exact Const.resKind_cases _
    | constApp2 h₁ h₂ _ _ =>
      have hk := (Typ.app hf ‹_› ‹_› hT).constApp2_kind h₁ h₂
      exact ⟨.hidden (Ty.opaque_of_kind (.inr hk)), by simp [Tm.head]⟩
  | _ => intro hv _ _ hJ; cases hJ <;> cases hv

end Flick
