import Formal.Live.UseStage

/-!
# Structural laws for typing by use

`LeLive` compares effective use stages, including qualifiers.
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
  | hide _ _ _ _ ha => exact absurd ha Ty.arrow_ne_tcon
  | data _ _ _ _ _ ha => exact absurd ha Ty.arrow_ne_tcon

theorem LeLive.arrow_right {a φ t C T} (h : LeLive S T (.arrow a φ t C)) :
    ∃ a' φ' t' C', T = .arrow a' φ' t' C' ∧ LeLive S a a' ∧
      (Stage.use φ' C').le (Stage.use φ C) ∧ LeLive S t' t ∧
      Caps.Sub T.reach (Ty.arrow a φ t C).reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, .refl, Stage.le_refl _, .refl, Caps.Sub.refl _⟩
  | arrow h₁ h₂ h₃ h₄ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃, h₄⟩
  | hide _ _ _ _ _ hb => exact absurd hb Ty.arrow_ne_tcon
  | data _ _ _ _ _ _ hb => exact absurd hb Ty.arrow_ne_tcon

theorem LeLive.eff_left {r ε t C T} (h : LeLive S (.eff r ε t C) T) :
    ∃ r' ε' t' C', T = .eff r' ε' t' C' ∧
      (Stage.use (.rt r ε) C).le (Stage.use (.rt r' ε') C') ∧
      LeLive S t t' ∧ Caps.Sub (Ty.eff r ε t C).reach T.reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, Stage.le_refl _, .refl, Caps.Sub.refl _⟩
  | eff h₁ h₂ h₃ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃⟩
  | hide _ _ _ _ ha => exact absurd ha Ty.eff_ne_tcon
  | data _ _ _ _ _ ha => exact absurd ha Ty.eff_ne_tcon

theorem LeLive.eff_right {r ε t C T} (h : LeLive S T (.eff r ε t C)) :
    ∃ r' ε' t' C', T = .eff r' ε' t' C' ∧
      (Stage.use (.rt r' ε') C').le (Stage.use (.rt r ε) C) ∧
      LeLive S t' t ∧ Caps.Sub T.reach (Ty.eff r ε t C).reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, Stage.le_refl _, .refl, Caps.Sub.refl _⟩
  | eff h₁ h₂ h₃ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃⟩
  | hide _ _ _ _ _ hb => exact absurd hb Ty.eff_ne_tcon
  | data _ _ _ _ _ _ hb => exact absurd hb Ty.eff_ne_tcon

theorem LeLive.rcons_left {l t r T} (h : LeLive S (.rcons l t r) T) :
    ∃ t' r', T = .rcons l t' r' ∧ LeLive S t t' ∧ LeLive S r r' ∧
      Caps.Sub (Ty.rcons l t r).reach T.reach := by
  cases h with
  | refl => exact ⟨_, _, rfl, .refl, .refl, Caps.Sub.refl _⟩
  | rcons h₁ h₂ h₃ => exact ⟨_, _, rfl, h₁, h₂, h₃⟩
  | hide _ _ _ _ ha => exact absurd ha Ty.rcons_ne_tcon
  | data _ _ _ _ _ ha => exact absurd ha Ty.rcons_ne_tcon

theorem LeLive.rcons_right {l t r T} (h : LeLive S T (.rcons l t r)) :
    ∃ t' r', T = .rcons l t' r' ∧ LeLive S t' t ∧ LeLive S r' r ∧
      Caps.Sub T.reach (Ty.rcons l t r).reach := by
  cases h with
  | refl => exact ⟨_, _, rfl, .refl, .refl, Caps.Sub.refl _⟩
  | rcons h₁ h₂ h₃ => exact ⟨_, _, rfl, h₁, h₂, h₃⟩
  | hide _ _ _ _ _ hb => exact absurd hb Ty.rcons_ne_tcon
  | data _ _ _ _ _ _ hb => exact absurd hb Ty.rcons_ne_tcon

/-- A record compares field by field; what it reaches follows. -/
theorem LeLive.rcons' {l t t' r r'} (ht : LeLive S t t') (hr : LeLive S r r') :
    LeLive S (.rcons l t r) (.rcons l t' r') :=
  .rcons ht hr fun x hx => hx.elim (fun h => .inl (ht.reach x h)) (fun h => .inr (hr.reach x h))

/-- A plain type has no other subtype or supertype. -/
theorem LeLive.eq_plain {a b : Ty} (h : LeLive S a b) (ha : a.isPlain = true) : b = a := by
  cases h with
  | refl => rfl
  | arrow => simp [Ty.isPlain] at ha
  | eff => simp [Ty.isPlain] at ha
  | rcons => simp [Ty.isPlain] at ha
  | hide _ _ _ _ ha' => exact absurd ha' (Ty.ne_tcon_of_plain ha)
  | data _ _ _ _ _ ha' => exact absurd ha' (Ty.ne_tcon_of_plain ha)

/-- A plain type has no other subtype. -/
theorem LeLive.eq_plain_right {a b : Ty} (h : LeLive S a b) (hb : b.isPlain = true) : a = b := by
  cases h with
  | refl => rfl
  | arrow => simp [Ty.isPlain] at hb
  | eff => simp [Ty.isPlain] at hb
  | rcons => simp [Ty.isPlain] at hb
  | hide _ _ _ _ _ hb' => exact absurd hb' (Ty.ne_tcon_of_plain hb)
  | data _ _ _ _ _ _ hb' => exact absurd hb' (Ty.ne_tcon_of_plain hb)

/-- Widening a hidden row, then anything. -/
theorem LeLive.hide_trans {T : Name} {i : Nat} {us us' : List Ty} {c : Ty}
    (hi : S.hidden T = some i) (hl : us.length = us'.length)
    (he : ∀ j, j ≠ i → us[j]? = us'[j]?)
    (hr : ∀ r r', us[i]? = some r → us'[i]? = some r' → Caps.Sub r.reach r'.reach)
    (h₂ : LeLive S (Ty.tcon T us') c) : LeLive S (Ty.tcon T us) c := by
  generalize hx : Ty.tcon T us' = x at h₂
  cases h₂ with
  | refl => subst hx; exact .hide hi hl he hr rfl rfl
  | arrow => exact absurd hx.symm Ty.arrow_ne_tcon
  | eff => exact absurd hx.symm Ty.eff_ne_tcon
  | rcons => exact absurd hx.symm Ty.rcons_ne_tcon
  | hide hi' hl' he' hr' ha hb =>
    subst hb
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq (hx.trans ha)
    rw [hi] at hi'; cases hi'
    refine .hide hi (hl.trans hl') (fun j hj => (he j hj).trans (he' j hj)) ?_ rfl rfl
    intro r r' h h'
    obtain ⟨m, hm⟩ : ∃ m, us'[i]? = some m := by
      have : i < us.length := (List.getElem?_eq_some_iff.mp h).1
      exact ⟨us'[i]'(hl ▸ this), by simp⟩
    exact (hr r m h hm).trans (hr' m r' hm h')
  | data hl' hr' hp' hn' hi' ha hb =>
    subst hb
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq (hx.trans ha)
    have hs := LeLive.hide_args he hr
    refine .data (hl.trans hl') (fun j u u' h h' => ?_) (fun j u u' h h' hj hv => ?_)
      (fun j u u' h h' hj hv => ?_) (fun j u u' h h' hj hv => ?_) rfl rfl
    all_goals
      obtain ⟨m, hm⟩ : ∃ m, us'[j]? = some m := by
        have : j < us.length := (List.getElem?_eq_some_iff.mp h).1
        exact ⟨us'[j]'(hl ▸ this), by simp⟩
    · exact (hs j u m h hm).trans (hr' j m u' hm h')
    all_goals
      have hji : j ≠ i := fun e => hj (e ▸ hi)
      rw [he j hji, hm] at h; cases h
    · exact hp' j _ u' hm h' hj hv
    · exact hn' j _ u' hm h' hj hv
    · exact hi' j _ u' hm h' hj hv

theorem Ty.sizeOf_lt_foldl : ∀ {us : List Ty} {f : Ty},
    sizeOf f < sizeOf (us.foldl .tapp f) + 1 ∧
    ∀ {j : Nat} {u : Ty}, us[j]? = some u → sizeOf u < sizeOf (us.foldl .tapp f)
  | [], f => ⟨by simp, by simp⟩
  | x :: xs, f => by
    obtain ⟨h1, h2⟩ := Ty.sizeOf_lt_foldl (us := xs) (f := Ty.tapp f x)
    simp only [List.foldl] at h1 h2 ⊢
    have e : sizeOf (Ty.tapp f x) = 1 + sizeOf f + sizeOf x := by simp
    have hx : sizeOf x < sizeOf (xs.foldl Ty.tapp (Ty.tapp f x)) := by omega
    refine ⟨by omega, fun {j u} h => ?_⟩
    cases j with
    | zero => simp at h; subst h; exact hx
    | succ j => exact h2 (by simpa using h)

theorem Ty.sizeOf_lt_tcon {T : Name} {us : List Ty} {j : Nat} {u : Ty} (h : us[j]? = some u) :
    sizeOf u < sizeOf (Ty.tcon T us) := (Ty.sizeOf_lt_foldl).2 h

/-- Two steps of data, argument by argument, given transitivity below the middle type. -/
theorem LeLive.data_trans {T : Name} {us vs : List Ty} {c : Ty}
    (ih : ∀ {m a c : Ty}, sizeOf m < sizeOf (Ty.tcon T vs) → LeLive S a m → LeLive S m c →
      LeLive S a c)
    (hl : us.length = vs.length)
    (hr : ∀ (j : Nat) (u u' : Ty), us[j]? = some u → vs[j]? = some u' → Caps.Sub u.reach u'.reach)
    (hp : ∀ (j : Nat) (u u' : Ty), us[j]? = some u → vs[j]? = some u' → S.hidden T ≠ some j →
      S.var T j = .pos → LeLive S u u')
    (hn : ∀ (j : Nat) (u u' : Ty), us[j]? = some u → vs[j]? = some u' → S.hidden T ≠ some j →
      S.var T j = .neg → LeLive S u' u)
    (hi : ∀ (j : Nat) (u u' : Ty), us[j]? = some u → vs[j]? = some u' → S.hidden T ≠ some j →
      S.var T j = .inv → u = u')
    (h₂ : LeLive S (Ty.tcon T vs) c) : LeLive S (Ty.tcon T us) c := by
  generalize hx : Ty.tcon T vs = x at h₂
  cases h₂ with
  | refl => subst hx; exact .data hl hr hp hn hi rfl rfl
  | arrow => exact absurd hx.symm Ty.arrow_ne_tcon
  | eff => exact absurd hx.symm Ty.eff_ne_tcon
  | rcons => exact absurd hx.symm Ty.rcons_ne_tcon
  | hide hi' hl' he' hr' ha hb =>
    subst hb
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq (hx.trans ha)
    refine .data (hl.trans hl') (fun j u u' h h' => ?_) (fun j u u' h h' hj hv => ?_)
      (fun j u u' h h' hj hv => ?_) (fun j u u' h h' hj hv => ?_) rfl rfl
    all_goals
      obtain ⟨m, hm⟩ : ∃ m, vs[j]? = some m := by
        have : j < us.length := (List.getElem?_eq_some_iff.mp h).1
        exact ⟨vs[j]'(hl ▸ this), by simp⟩
    · exact (hr j u m h hm).trans (LeLive.hide_args he' hr' j m u' hm h')
    all_goals
      have hji : j ≠ _ := fun e => hj (e ▸ hi')
      have hv' : vs[j]? = some u' := (he' j hji).trans h'
    · exact hp j u u' h hv' hj hv
    · exact hn j u u' h hv' hj hv
    · exact hi j u u' h hv' hj hv
  | data hl' hr' hp' hn' hi' ha hb =>
    subst hb
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq (hx.trans ha)
    refine .data (hl.trans hl') (fun j u u' h h' => ?_) (fun j u u' h h' hj hv => ?_)
      (fun j u u' h h' hj hv => ?_) (fun j u u' h h' hj hv => ?_) rfl rfl
    all_goals
      obtain ⟨m, hm⟩ : ∃ m, vs[j]? = some m := by
        have : j < us.length := (List.getElem?_eq_some_iff.mp h).1
        exact ⟨vs[j]'(hl ▸ this), by simp⟩
    · exact (hr j u m h hm).trans (hr' j m u' hm h')
    · exact ih (Ty.sizeOf_lt_tcon hm) (hp j u m h hm hj hv) (hp' j m u' hm h' hj hv)
    · exact ih (Ty.sizeOf_lt_tcon hm) (hn' j m u' hm h' hj hv) (hn j u m h hm hj hv)
    · exact (hi j u m h hm hj hv).trans (hi' j m u' hm h' hj hv)

theorem LeLive.trans_lt (n : Nat) : ∀ {b a c : Ty}, sizeOf b < n → LeLive S a b →
    LeLive S b c → LeLive S a c := by
  induction n with
  | zero => intro b a c hn; exact absurd hn (Nat.not_lt_zero _)
  | succ n ihn =>
    intro b a c hn h₁ h₂
    have ih : ∀ {m a c : Ty}, sizeOf m < sizeOf b → LeLive S a m → LeLive S m c →
        LeLive S a c := fun hm => ihn (by omega)
    by_cases hA : ∃ x φ y C, b = .arrow x φ y C
    · obtain ⟨x, φ, y, C, rfl⟩ := hA
      obtain ⟨_, _, _, _, rfl, ha, hφ, ht, hc⟩ := h₁.arrow_right
      obtain ⟨_, _, _, _, rfl, ga, gφ, gt, gc⟩ := h₂.arrow_left
      exact .arrow (ih (by simp; omega) ga ha) (Stage.le_trans hφ gφ)
        (ih (by simp; omega) ht gt) (hc.trans gc)
    by_cases hE : ∃ r ε t C, b = .eff r ε t C
    · obtain ⟨r, ε, t, C, rfl⟩ := hE
      obtain ⟨_, _, _, _, rfl, hφ, ht, hc⟩ := h₁.eff_right
      obtain ⟨_, _, _, _, rfl, gφ, gt, gc⟩ := h₂.eff_left
      exact .eff (Stage.le_trans hφ gφ) (ih (by first | simp | (simp; omega)) ht gt)
        (hc.trans gc)
    by_cases hR : ∃ l t r, b = .rcons l t r
    · obtain ⟨l, t, r, rfl⟩ := hR
      obtain ⟨_, _, rfl, ht, hr, hc⟩ := h₁.rcons_right
      obtain ⟨_, _, rfl, gt, gr, gc⟩ := h₂.rcons_left
      exact .rcons (ih (by simp; omega) ht gt) (ih (by simp; omega) hr gr) (hc.trans gc)
    cases h₁ with
    | refl => exact h₂
    | arrow => exact absurd ⟨_, _, _, _, rfl⟩ hA
    | eff => exact absurd ⟨_, _, _, _, rfl⟩ hE
    | rcons => exact absurd ⟨_, _, _, rfl⟩ hR
    | hide hi hl he hr ha hb => subst ha; rw [hb] at h₂; exact LeLive.hide_trans hi hl he hr h₂
    | data hl hr hp hneg hinv ha hb =>
      subst ha; subst hb
      exact LeLive.data_trans ih hl hr hp hneg hinv h₂

theorem LeLive.trans {a b c : Ty} (h₁ : LeLive S a b) (h₂ : LeLive S b c) : LeLive S a c :=
  LeLive.trans_lt (sizeOf b + 1) (by omega) h₁ h₂

theorem LeLive.substT {a b : Ty} (σ : Atom → Ty) (h : LeLive S a b) :
    LeLive S (a.substT σ) (b.substT σ) := by
  have sub : ∀ {x y : Ty}, Caps.Sub x.reach y.reach →
      Caps.Sub (x.substT σ).reach (y.substT σ).reach := fun hc a ha => by
    obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ a).mp ha
    exact (Ty.reach_substT σ _ a).mpr ⟨b, hc b hb, h'⟩
  induction h with
  | refl => exact .refl
  | arrow _ hφ _ hc ih₁ ih₂ =>
    refine .arrow ih₁ ?_ ih₂ (sub hc)
    simpa only [Stage.use_substT] using Stage.le_substT σ hφ
  | eff hφ _ hc ih =>
    refine .eff ?_ ih (sub hc)
    simpa only [Stage.use_substT] using Stage.le_substT σ hφ
  | rcons _ _ hc ih₁ ih₂ => exact .rcons ih₁ ih₂ (sub hc)
  | hide hi hl he hr ha hb =>
    subst ha; subst hb
    rw [Ty.tcon_substT, Ty.tcon_substT]
    refine .hide hi (by simpa using hl) (fun j hj => by simp [he j hj]) ?_ rfl rfl
    intro r r' h h'
    simp only [List.getElem?_map, Option.map_eq_some_iff] at h h'
    obtain ⟨u, hu, rfl⟩ := h
    obtain ⟨u', hu', rfl⟩ := h'
    exact sub (hr u u' hu hu')
  | data hl hr _ _ hinv ha hb ihp ihn =>
    subst ha; subst hb
    rw [Ty.tcon_substT, Ty.tcon_substT]
    refine .data (by simpa using hl) (fun j x x' h h' => ?_) (fun j x x' h h' hj hv => ?_)
      (fun j x x' h h' hj hv => ?_) (fun j x x' h h' hj hv => ?_) rfl rfl
    all_goals
      simp only [List.getElem?_map, Option.map_eq_some_iff] at h h'
      obtain ⟨u, hu, rfl⟩ := h
      obtain ⟨u', hu', rfl⟩ := h'
    · exact sub (hr j u u' hu hu')
    · exact ihp j u u' hu hu' hj hv
    · exact ihn j u u' hu hu' hj hv
    · rw [hinv j u u' hu hu' hj hv]

theorem LeLiveI.trans {a b c : Ty} (h : LeLiveI S a b)
    (hc : ∀ {x}, LeLive S x b → LeLive S x c) : LeLiveI S a c := by
  induction h with
  | le h => exact .le (hc h)
  | inst hus _ ih => exact .inst hus (ih hc)

theorem LeLiveI.substT {a b : Ty} (σ : Atom → Ty) (h : LeLiveI S a b) :
    LeLiveI S (a.substT σ) (b.substT σ) := by
  induction h with
  | le h => exact .le (h.substT σ)
  | @inst k t T us hus _ ih =>
    subst hus
    refine .inst (us := us.map (Ty.substT σ)) (by simp) ?_
    rw [Ty.inst_substT]; exact ih

theorem Row.outlives_substT {Θ r s} (σ : Atom → Ty)
    (h : Row.Outlives Θ r s) : Row.Outlives (Θ.substT σ) (r.substT σ) (s.substT σ) := by
  simpa [Row.Outlives, Ty.reach_caps, Ty.substT] using h.substT σ

theorem Const.ok_substT {Θ c ι} (σ : Atom → Ty)
    (h : Const.ok Θ c ι) : Const.ok (Θ.substT σ) c (ι.substT σ) := by
  cases c <;> simp only [Const.ok, CArg.substT] at h ⊢
  case addFin =>
    exact ⟨h.1.substT σ, Row.outlives_substT σ h.2.1, by rw [← Row.ofCaps_substT]; exact h.2.2.substT σ⟩
  case spawn =>
    refine ⟨?_, Row.outlives_substT σ h.2.1, by rw [← Row.ofCaps_substT]; exact h.2.2.substT σ⟩
    have hs := h.1.substT σ
    intro a ha
    apply hs a
    rcases ha with ⟨b, hb, h'⟩ | ha
    · exact ⟨b, .inl hb, h'⟩
    · obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ a).mp ha
      exact ⟨b, .inr hb, h'⟩
  case scopeRun => rw [← Row.ofCaps_substT]; exact h.substT σ
  case refMake | refGet | refSet => rw [← Ty.reach_caps]; exact h.substT σ
  all_goals exact h

theorem Const.ok_mono {Θ Θ' c ι} (h : Const.ok Θ c ι)
    (hΘ : ∀ C s, Outl Θ C s → Outl Θ' C s) : Const.ok Θ' c ι := by
  cases c <;> simp only [Const.ok] at h ⊢
  case addFin | spawn => exact ⟨hΘ _ _ h.1, hΘ _ _ h.2.1, h.2.2⟩
  case refMake | refGet | refSet => exact hΘ _ _ h
  all_goals exact h

end Flick
