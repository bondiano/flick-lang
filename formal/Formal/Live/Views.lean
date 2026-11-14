import Formal.Live.UseStage
import Formal.Live.Equivalence

/-!
# Type formers and reach through subtyping by use
-/

namespace Flick

theorem Ty.reach_foldl_le : ∀ {us us' : List Ty} {f f' : Ty}, us.length = us'.length →
    Caps.Sub f.reach f'.reach →
    (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' → Caps.Sub u.reach u'.reach) →
    Caps.Sub (us.foldl .tapp f).reach (us'.foldl .tapp f').reach
  | [], [], _, _, _, hf, _ => hf
  | x :: xs, y :: ys, f, f', hl, hf, hu => by
    simp only [List.foldl]
    refine Ty.reach_foldl_le (by simpa using hl) ?_
      (fun j u u' h h' => hu (j + 1) u u' (by simpa using h) (by simpa using h'))
    intro a ha
    simp only [Ty.reach] at ha ⊢
    rcases ha with ha | ha
    · exact .inl (hf a ha)
    · exact .inr (hu 0 x y rfl rfl a ha)
  | [], _ :: _, _, _, hl, _, _ => by simp at hl
  | _ :: _, [], _, _, hl, _, _ => by simp at hl

/-- The arguments of a widened hidden row compare by reach, each of them. -/
theorem LeLive.hide_args {i : Nat} {us us' : List Ty}
    (he : ∀ j, j ≠ i → us[j]? = us'[j]?)
    (hr : ∀ r r', us[i]? = some r → us'[i]? = some r' → Caps.Sub r.reach r'.reach) :
    ∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' → Caps.Sub u.reach u'.reach := by
  intro j u u' h h'
  by_cases hj : j = i
  · subst hj; exact hr u u' h h'
  · rw [he j hj, h'] at h; cases h; exact Caps.Sub.refl _

theorem LeLive.reach {a b : Ty} (h : LeLive S a b) : Caps.Sub a.reach b.reach := by
  cases h with
  | refl => exact Caps.Sub.refl _
  | equiv he => exact fun x hx => Eq.mp (congrFun he.reach x) hx
  | arrow _ _ _ hc | eff _ _ hc | rcons _ _ hc => exact hc
  | hide _ hl he hr ha hb =>
    subst ha; subst hb
    exact Ty.reach_foldl_le hl (Caps.Sub.refl _) (LeLive.hide_args he hr)
  | data hl hr _ _ _ ha hb =>
    subst ha; subst hb
    exact Ty.reach_foldl_le hl (Caps.Sub.refl _) hr

theorem LeLive.arrow_left {a φ t C T} (h : LeLive S (.arrow a φ t C) T) :
    ∃ a' φ' t' C', T = .arrow a' φ' t' C' ∧ LeLive S a' a ∧
      (Stage.use φ C).le (Stage.use φ' C') ∧ LeLive S t t' ∧
      Caps.Sub (Ty.arrow a φ t C).reach T.reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, .refl, Stage.le_refl _, .refl, Caps.Sub.refl _⟩
  | arrow h₁ h₂ h₃ h₄ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃, h₄⟩
  | equiv he =>
    cases he.form with
    | rows hr _ => cases hr
    | arrow ha ht => exact ⟨_, _, _, _, rfl, .equiv ha.symm, Stage.le_refl _,
        .equiv ht, fun x hx => Eq.mp (congrFun he.reach x) hx⟩
  | hide _ _ _ _ ha => exact absurd ha Ty.arrow_ne_tcon
  | data _ _ _ _ _ ha => exact absurd ha Ty.arrow_ne_tcon

theorem LeLive.arrow_right {a φ t C T} (h : LeLive S T (.arrow a φ t C)) :
    ∃ a' φ' t' C', T = .arrow a' φ' t' C' ∧ LeLive S a a' ∧
      (Stage.use φ' C').le (Stage.use φ C) ∧ LeLive S t' t ∧
      Caps.Sub T.reach (Ty.arrow a φ t C).reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, .refl, Stage.le_refl _, .refl, Caps.Sub.refl _⟩
  | arrow h₁ h₂ h₃ h₄ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃, h₄⟩
  | equiv he =>
    cases he.form with
    | rows _ hr => cases hr
    | arrow ha ht => exact ⟨_, _, _, _, rfl, .equiv ha.symm, Stage.le_refl _,
        .equiv ht, fun x hx => Eq.mp (congrFun he.reach x) hx⟩
  | hide _ _ _ _ _ hb => exact absurd hb Ty.arrow_ne_tcon
  | data _ _ _ _ _ _ hb => exact absurd hb Ty.arrow_ne_tcon

theorem LeLive.eff_left {r ε t C T} (h : LeLive S (.eff r ε t C) T) :
    ∃ r' ε' t' C', T = .eff r' ε' t' C' ∧
      (Stage.use (.rt r ε) C).le (Stage.use (.rt r' ε') C') ∧
      LeLive S t t' ∧ Caps.Sub (Ty.eff r ε t C).reach T.reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, Stage.le_refl _, .refl, Caps.Sub.refl _⟩
  | eff h₁ h₂ h₃ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃⟩
  | equiv he =>
    cases he.form with
    | rows hr _ => cases hr
    | eff ht => exact ⟨_, _, _, _, rfl, Stage.le_refl _, .equiv ht,
        fun x hx => Eq.mp (congrFun he.reach x) hx⟩
  | hide _ _ _ _ ha => exact absurd ha Ty.eff_ne_tcon
  | data _ _ _ _ _ ha => exact absurd ha Ty.eff_ne_tcon

theorem LeLive.eff_right {r ε t C T} (h : LeLive S T (.eff r ε t C)) :
    ∃ r' ε' t' C', T = .eff r' ε' t' C' ∧
      (Stage.use (.rt r' ε') C').le (Stage.use (.rt r ε) C) ∧
      LeLive S t' t ∧ Caps.Sub T.reach (Ty.eff r ε t C).reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, Stage.le_refl _, .refl, Caps.Sub.refl _⟩
  | eff h₁ h₂ h₃ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃⟩
  | equiv he =>
    cases he.form with
    | rows _ hr => cases hr
    | eff ht => exact ⟨_, _, _, _, rfl, Stage.le_refl _, .equiv ht,
        fun x hx => Eq.mp (congrFun he.reach x) hx⟩
  | hide _ _ _ _ _ hb => exact absurd hb Ty.eff_ne_tcon
  | data _ _ _ _ _ _ hb => exact absurd hb Ty.eff_ne_tcon

theorem LeLive.rcons_left {l t r T} (h : LeLive S (.rcons l t r) T) :
    ∃ t' r', T = .rcons l t' r' ∧ LeLive S t t' ∧ LeLive S r r' ∧
      Caps.Sub (Ty.rcons l t r).reach T.reach := by
  cases h with
  | refl => exact ⟨_, _, rfl, .refl, .refl, Caps.Sub.refl _⟩
  | rcons h₁ h₂ h₃ => exact ⟨_, _, rfl, h₁, h₂, h₃⟩
  | equiv he =>
    cases he.form with
    | rows hr _ => cases hr
    | rcons ht hr => exact ⟨_, _, rfl, .equiv ht, .equiv hr,
        fun x hx => Eq.mp (congrFun he.reach x) hx⟩
  | hide _ _ _ _ ha => exact absurd ha Ty.rcons_ne_tcon
  | data _ _ _ _ _ ha => exact absurd ha Ty.rcons_ne_tcon

theorem LeLive.rcons_right {l t r T} (h : LeLive S T (.rcons l t r)) :
    ∃ t' r', T = .rcons l t' r' ∧ LeLive S t' t ∧ LeLive S r' r ∧
      Caps.Sub T.reach (Ty.rcons l t r).reach := by
  cases h with
  | refl => exact ⟨_, _, rfl, .refl, .refl, Caps.Sub.refl _⟩
  | rcons h₁ h₂ h₃ => exact ⟨_, _, rfl, h₁, h₂, h₃⟩
  | equiv he =>
    cases he.form with
    | rows _ hr => cases hr
    | rcons ht hr => exact ⟨_, _, rfl, .equiv ht, .equiv hr,
        fun x hx => Eq.mp (congrFun he.reach x) hx⟩
  | hide _ _ _ _ _ hb => exact absurd hb Ty.rcons_ne_tcon
  | data _ _ _ _ _ _ hb => exact absurd hb Ty.rcons_ne_tcon

/-- A record compares field by field; what it reaches follows. -/
theorem LeLive.rcons' {l t t' r r'} (ht : LeLive S t t') (hr : LeLive S r r') :
    LeLive S (.rcons l t r) (.rcons l t' r') :=
  .rcons ht hr fun x hx => hx.elim (fun h => .inl (ht.reach x h)) (fun h => .inr (hr.reach x h))

/-- A plain type has only equivalent subtypes and supertypes. -/
theorem LeLive.eq_plain {a b : Ty} (h : LeLive S a b) (ha : a.isPlain = true) : Ty.Equiv b a := by
  cases h with
  | refl => exact .refl
  | equiv he => exact he.symm
  | arrow => simp [Ty.isPlain] at ha
  | eff => simp [Ty.isPlain] at ha
  | rcons => simp [Ty.isPlain] at ha
  | hide _ _ _ _ ha' => exact absurd ha' (Ty.ne_tcon_of_plain ha)
  | data _ _ _ _ _ ha' => exact absurd ha' (Ty.ne_tcon_of_plain ha)

/-- A subtype of a plain type is equivalent to it. -/
theorem LeLive.eq_plain_right {a b : Ty} (h : LeLive S a b) (hb : b.isPlain = true) : Ty.Equiv a b := by
  cases h with
  | refl => exact .refl
  | equiv he => exact he
  | arrow => simp [Ty.isPlain] at hb
  | eff => simp [Ty.isPlain] at hb
  | rcons => simp [Ty.isPlain] at hb
  | hide _ _ _ _ _ hb' => exact absurd hb' (Ty.ne_tcon_of_plain hb)
  | data _ _ _ _ _ _ hb' => exact absurd hb' (Ty.ne_tcon_of_plain hb)

end Flick
