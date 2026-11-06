import Formal.Rules

/-!
# Renaming and substitution keep values, matches and the coverage premise
-/

namespace Flick

variable {S : Sig}

theorem Tm.unguarded_rename (ξ : Nat → Nat) (A : Tm) : (A.rename ξ).unguarded = A.unguarded := by
  induction A <;> simp_all [Tm.rename, Tm.unguarded]

theorem Typ.unguarded_subst {Ψ Θ Γ A J} (h : Typ S Ψ Θ Γ A J) (σ : Nat → Tm) :
    ∀ {t t' φ}, J = .alts t t' φ → (A.subst σ).unguarded = A.unguarded := by
  induction h with
  | anil => intros; rfl
  | acons _ _ _ _ ih => intro _ _ _ hJ; simp [Tm.subst, Tm.unguarded, ih hJ]
  | aguard _ _ _ _ _ _ ih => intro _ _ _ hJ; simp [Tm.subst, Tm.unguarded, ih hJ]
  | aref _ _ _ _ _ _ _ _ _ ih => intro _ _ _ hJ; simp [Tm.subst, Tm.unguarded, ih hJ]
  | _ => intro _ _ _ hJ; cases hJ

theorem Tm.head_rename {ξ : Nat → Nat} {K} {f : Tm} (h : f.head = .ctor K) :
    (f.rename ξ).head = .ctor K := by
  induction f <;> simp_all [Tm.head, Tm.rename]

theorem Tm.head_subst {σ : Nat → Tm} {K} {f : Tm} (h : f.head = .ctor K) :
    (f.subst σ).head = .ctor K := by
  induction f <;> simp_all [Tm.head, Tm.subst]

/-- A map of terms that keeps every constructor values are made of. -/
structure Homo (m : Tm → Tm) : Prop where
  rcons : ∀ l e r, m (.rcons l e r) = .rcons l (m e) (m r)
  app : ∀ f e, m (.app f e) = .app (m f) (m e)
  lit : ∀ l, m (.lit l) = .lit l
  ctor : ∀ K, m (.ctor K) = .ctor K
  rnil : m .rnil = .rnil
  lam : ∀ b, ∃ b', m (.lam b) = .lam b'
  fix : ∀ b, ∃ b', m (.fix b) = .fix b'
  loc : ∀ l, m (.loc l) = .loc l
  const : ∀ c, m (.const c) = .const c
  task : ∀ e, ∃ e', m (.task e) = .task e'
  enil : m .enil = .enil
  econs : ∀ N v θ, m (.econs N v θ) = .econs N (m v) (m θ)
  cap : ∀ p, m (.cap p) = .cap p
  own : ∀ p, m (.own p) = .own p
  drec : ∀ c r, m (.drec c r) = .drec c (m r)
  kont : ∀ p ℓ, m (.kont p ℓ) = .kont p ℓ

theorem Homo.rename (ξ : Nat → Nat) : Homo (Tm.rename ξ) :=
  ⟨fun _ _ _ => rfl, fun _ _ => rfl, fun _ => rfl, fun _ => rfl, rfl, fun _ => ⟨_, rfl⟩,
    fun _ => ⟨_, rfl⟩, fun _ => rfl, fun _ => rfl, fun _ => ⟨_, rfl⟩, rfl, fun _ _ _ => rfl,
    fun _ => rfl, fun _ => rfl, fun _ _ => rfl, fun _ _ => rfl⟩

theorem Homo.subst (σ : Nat → Tm) : Homo (Tm.subst σ) :=
  ⟨fun _ _ _ => rfl, fun _ _ => rfl, fun _ => rfl, fun _ => rfl, rfl, fun _ => ⟨_, rfl⟩,
    fun _ => ⟨_, rfl⟩, fun _ => rfl, fun _ => rfl, fun _ => ⟨_, rfl⟩, rfl, fun _ _ _ => rfl,
    fun _ => rfl, fun _ => rfl, fun _ _ => rfl, fun _ _ => rfl⟩

theorem Homo.head {m : Tm → Tm} (hm : Homo m) {K} :
    ∀ {f : Tm}, Value f → f.head = .ctor K → (m f).head = .ctor K := by
  intro f hv h
  induction hv with
  | ctor => simp [Tm.head] at h; subst h; rw [hm.ctor]; rfl
  | ctorApp _ _ _ ih _ => rw [hm.app]; exact ih h
  | _ => simp [Tm.head] at h

theorem Homo.head_rev {m : Tm → Tm} (hm : Homo m) {K} :
    ∀ {f : Tm}, Value f → (m f).head = .ctor K → f.head = .ctor K := by
  intro f hv h
  induction hv with
  | ctor => rw [hm.ctor] at h; exact h
  | ctorApp _ _ _ ih _ => rw [hm.app] at h; exact ih h
  | constApp => rw [hm.app, hm.const] at h; simp [Tm.head] at h
  | constApp2 => rw [hm.app, hm.app, hm.const] at h; simp [Tm.head] at h
  | lam => obtain ⟨_, e⟩ := hm.lam _; rw [e] at h; simp [Tm.head] at h
  | fix => obtain ⟨_, e⟩ := hm.fix _; rw [e] at h; simp [Tm.head] at h
  | task => obtain ⟨_, e⟩ := hm.task _; rw [e] at h; simp [Tm.head] at h
  | lit => rw [hm.lit] at h; simp [Tm.head] at h
  | loc => rw [hm.loc] at h; simp [Tm.head] at h
  | const => rw [hm.const] at h; simp [Tm.head] at h
  | rnil => rw [hm.rnil] at h; simp [Tm.head] at h
  | enil => rw [hm.enil] at h; simp [Tm.head] at h
  | rcons => rw [hm.rcons] at h; simp [Tm.head] at h
  | econs => rw [hm.econs] at h; simp [Tm.head] at h
  | cap => rw [hm.cap] at h; simp [Tm.head] at h
  | own => rw [hm.own] at h; simp [Tm.head] at h
  | drec => rw [hm.drec] at h; simp [Tm.head] at h
  | kont => rw [hm.kont] at h; simp [Tm.head] at h

theorem Homo.value {m : Tm → Tm} (hm : Homo m) {e} (h : Value e) : Value (m e) := by
  induction h with
  | lam => obtain ⟨_, h⟩ := hm.lam _; rw [h]; exact .lam
  | fix => obtain ⟨_, h⟩ := hm.fix _; rw [h]; exact .fix
  | task => obtain ⟨_, h⟩ := hm.task _; rw [h]; exact .task
  | lit => rw [hm.lit]; exact .lit
  | loc => rw [hm.loc]; exact .loc
  | const => rw [hm.const]; exact .const
  | ctor => rw [hm.ctor]; exact .ctor
  | rnil => rw [hm.rnil]; exact .rnil
  | enil => rw [hm.enil]; exact .enil
  | cap => rw [hm.cap]; exact .cap
  | own => rw [hm.own]; exact .own
  | drec => rw [hm.drec]; exact .drec
  | kont => rw [hm.kont]; exact .kont
  | rcons _ _ ih₁ ih₂ => rw [hm.rcons]; exact .rcons ih₁ ih₂
  | econs _ _ ih₁ ih₂ => rw [hm.econs]; exact .econs ih₁ ih₂
  | ctorApp hf hK _ ih₁ ih₂ => rw [hm.app]; exact .ctorApp ih₁ (hm.head hf hK) ih₂
  | constApp hc _ ih => rw [hm.app, hm.const]; exact .constApp hc ih
  | constApp2 h₁ h₂ _ _ ih₁ ih₂ => rw [hm.app, hm.app, hm.const]; exact .constApp2 h₁ h₂ ih₁ ih₂

theorem Homo.isEnv {m : Tm → Tm} (hm : Homo m) {θ} (h : IsEnv θ) : IsEnv (m θ) := by
  induction h with
  | enil => rw [hm.enil]; exact .enil
  | econs hv _ ih => rw [hm.econs]; exact .econs (hm.value hv) ih

theorem Value.rename {ξ : Nat → Nat} {e} (h : Value e) : Value (e.rename ξ) :=
  (Homo.rename ξ).value h

theorem Value.subst {σ : Nat → Tm} {e} (h : Value e) : Value (e.subst σ) :=
  (Homo.subst σ).value h

theorem Tm.field_map {m : Tm → Tm} (hm : Homo m) {l} :
    ∀ {e x : Tm}, e.field l = some x → (m e).field l = some (m x) := by
  intro e x h
  induction e with
  | rcons l' e r _ ih =>
    rw [hm.rcons]
    simp only [Tm.field] at h ⊢
    split at h
    · simp_all
    · simp_all
  | drec c r ih => rw [hm.drec]; simp only [Tm.field] at h ⊢; exact ih h
  | _ => simp [Tm.field] at h

theorem matchP_app {p q f e δ} : matchP (.app p q) (.app f e) = some δ ↔
    ∃ δ₁ δ₂, matchP p f = some δ₁ ∧ matchP q e = some δ₂ ∧ δ₁ ++ δ₂ = δ := by
  simp [matchP, Option.bind_eq_some_iff]

theorem matchP_rfield {l p r v δ} : matchP (.rfield l p r) v = some δ ↔
    ∃ x δ₁ δ₂, v.field l = some x ∧ matchP p x = some δ₁ ∧ matchP r v = some δ₂ ∧
      δ₁ ++ δ₂ = δ := by
  simp [matchP, Option.bind_eq_some_iff]

theorem matchP_or {p q v δ} : matchP (.or p q) v = some δ ↔
    matchP p v = some δ ∨ (matchP p v = none ∧ matchP q v = some δ) := by
  simp only [matchP]; cases matchP p v <;> simp

/-- A map that keeps the constructors patterns inspect keeps every match. -/
theorem matchP_map {m : Tm → Tm} (hm : Homo m) :
    ∀ (p : Pat) {e : Tm}, (matchP p e).isSome → (matchP p (m e)).isSome
  | .wild, _, _ => by simp [matchP]
  | .var, _, _ => by simp [matchP]
  | .lit l, e, h => by
    cases e <;> simp [matchP] at h
    subst h; rw [hm.lit]; simp [matchP]
  | .ctor K, e, h => by
    cases e <;> simp [matchP] at h
    subst h; rw [hm.ctor]; simp [matchP]
  | .app p q, e, h => by
    cases e with
    | app f a =>
      obtain ⟨δ, hδ⟩ := Option.isSome_iff_exists.mp h
      obtain ⟨δ₁, δ₂, h1, h2, -⟩ := matchP_app.mp hδ
      obtain ⟨d₁, e₁⟩ := Option.isSome_iff_exists.mp (matchP_map hm p (e := f) (by simp [h1]))
      obtain ⟨d₂, e₂⟩ := Option.isSome_iff_exists.mp (matchP_map hm q (e := a) (by simp [h2]))
      rw [hm.app]; simp [matchP, e₁, e₂]
    | _ => simp [matchP] at h
  | .rany, e, h => by
    cases e <;> simp [matchP] at h
    · rw [hm.rnil]; simp [matchP]
    · rw [hm.rcons]; simp [matchP]
  | .rfield l p r, e, h => by
    obtain ⟨δ, hδ⟩ := Option.isSome_iff_exists.mp h
    obtain ⟨x, δ₁, δ₂, hx, h1, h2, -⟩ := matchP_rfield.mp hδ
    obtain ⟨a', ha'⟩ := Option.isSome_iff_exists.mp (matchP_map hm p (e := x) (by simp [h1]))
    obtain ⟨b', hb'⟩ := Option.isSome_iff_exists.mp (matchP_map hm r (e := e) (by simp [h2]))
    simp [matchP, Tm.field_map hm hx, ha', hb']
  | .or p q, e, h => by
    simp only [matchP, Option.isSome_or] at h ⊢
    rcases Bool.or_eq_true_iff.mp h with h | h
    · simp [matchP_map hm p h]
    · simp [matchP_map hm q h]

theorem Covers.map {m : Tm → Tm} (hm : Homo m) {t ps e} (h : Covers S t ps e) :
    Covers S t ps (m e) :=
  h.imp id fun ⟨hv, p, hp, hmt⟩ => ⟨hm.value hv, p, hp, matchP_map hm p hmt⟩

/-- The `try` coverage of a value is about its head constructor. -/
theorem HCov.map {m : Tm → Tm} (hm : Homo m) {v H ε'} (hv : Value v) (h : HCov S v H ε') :
    HCov S (m v) H ε' := by
  intro K k ts T hK hS
  exact h K k ts T (hm.head_rev hv hK) hS

end Flick
