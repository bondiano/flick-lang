import Formal.Widen

/-!
# Varying a parameter of data by its variance

Subtyping compares the arguments of data by the variance of each parameter
(regions.md §2.2.1, `LeLive.data`). A field holds a parameter only where its variance allows
(`Ty.Pol`), so a field built at one argument is below the field read at the other
(`field_vary`): at the field's own polarity, in the direction the parameter allows.
-/

namespace Flick

variable {S : Sig}

theorem Ty.NoV.arrow {i : Nat} {a b : Ty} {φ : Stage} {C : Caps} (ha : a.NoV i)
    (hφ : φ.NoV i) (hb : b.NoV i) (hC : (Ty.caps C).NoV i) : (Ty.arrow a φ b C).NoV i := by
  simp only [Ty.NoV, Ty.substT] at ha hb hC ⊢
  simp only [Ty.caps.injEq] at hC
  rw [ha, hφ, hb, hC]

theorem Ty.NoV.eff {i : Nat} {r e : Row} {t : Ty} {C : Caps} (hr : (Ty.row r).NoV i)
    (he : (Ty.row e).NoV i) (ht : t.NoV i) (hC : (Ty.caps C).NoV i) :
    (Ty.eff r e t C).NoV i := by
  simp only [Ty.NoV, Ty.substT, Ty.row.injEq, Ty.caps.injEq] at hr he ht hC ⊢
  rw [hr, he, ht, hC]

theorem Ty.NoV.rcons {i : Nat} {l : Label} {t r : Ty} (ht : t.NoV i) (hr : r.NoV i) :
    (Ty.rcons l t r).NoV i := by
  simp only [Ty.NoV, Ty.substT] at ht hr ⊢
  rw [ht, hr]

theorem Ty.NoV.tcon {i : Nat} {T : Name} {us : List Ty}
    (h : ∀ (j : Nat) (u : Ty), us[j]? = some u → u.NoV i) : (Ty.tcon T us).NoV i := by
  simp only [Ty.NoV, Ty.tcon_substT]
  congr 1
  apply List.ext_getElem?
  intro j
  simp only [List.getElem?_map]
  cases hu : us[j]? with
  | none => rfl
  | some u => simp only [Option.map_some]; exact congrArg some (h j u hu)

/-- A parameter that is not covariant at this polarity is not held at all. -/
theorem Ty.Pol.noV {i : Nat} {s : Variance} {q : Bool} {f : Ty}
    (h : Ty.Pol S.var i s q f) (hs : ¬ s.at q ∧ ¬ s.at (!q) ∨ (s = .pos ∧ q = false)) :
    f.NoV i := by
  induction h with
  | none h => exact h
  | var hq =>
    rcases hs with ⟨h1, -⟩ | ⟨rfl, rfl⟩
    · exact absurd hq h1
    · simp [Variance.at] at hq
  | arrow hpa _ hφ _ hC iha ihb =>
    rcases hs with ⟨h1, h2⟩ | ⟨rfl, rfl⟩
    · refine Ty.NoV.arrow (iha (.inl ⟨h2, by simpa using h1⟩)) hφ (ihb (.inl ⟨h1, h2⟩)) ?_
      rcases hC with hC | hC
      · exact hC
      · exact absurd hC h1
    · refine Ty.NoV.arrow (hpa rfl) hφ (ihb (.inr ⟨rfl, rfl⟩)) ?_
      rcases hC with hC | hC
      · exact hC
      · simp [Variance.at] at hC
  | eff hr he _ hC iht =>
    refine Ty.NoV.eff hr he (iht hs) ?_
    rcases hC with hC | hC
    · exact hC
    · rcases hs with ⟨h1, -⟩ | ⟨rfl, rfl⟩
      · exact absurd hC h1
      · simp [Variance.at] at hC
  | @data q t T us _ hng _ hin hbi ht ihp ihn =>
    subst ht
    apply Ty.NoV.tcon
    intro j u hu
    cases hv : S.var T j with
    | pos => exact ihp j u hu hv hs
    | neg =>
      rcases hs with ⟨h1, h2⟩ | ⟨rfl, rfl⟩
      · exact ihn j u hu hv (.inl ⟨h2, by simpa using h1⟩)
      · exact hng j u hu hv rfl
    | inv => exact hin j u hu hv
    | bi =>
      rcases hs with ⟨h1, -⟩ | ⟨rfl, rfl⟩
      · exact hbi j u hu hv h1
      · exact hbi j u hu hv (by simp [Variance.at])

theorem Variance.noV_cond {s : Variance} (h : s ≠ .neg) :
    ¬ s.at false ∧ ¬ s.at (!false) ∨ (s = .pos ∧ false = false) := by
  cases s with
  | pos => exact .inr ⟨rfl, rfl⟩
  | neg => exact absurd rfl h
  | inv | bi => exact .inl ⟨by simp [Variance.at], by simp [Variance.at]⟩

/-- A field built at one argument is below the field read at the other, at its own
polarity, when the parameter changes in the direction its variance allows and keeps what
it reaches. -/
theorem field_vary {i : Nat} {s : Variance} {q : Bool} {f : Ty} (hf : Ty.Pol S.var i s q f)
    {σ σ' : Atom → Ty} (hσ : ∀ a, a ≠ .v i → σ a = σ' a)
    (hr : Caps.Sub (σ (.v i)).reach (σ' (.v i)).reach)
    (hp : s = .pos → LeLive S (σ (.v i)) (σ' (.v i)))
    (hn : s = .neg → LeLive S (σ' (.v i)) (σ (.v i))) :
    (q = true → LeLive S (f.substT σ) (f.substT σ')) ∧
    (q = false → LeLive S (f.substT σ') (f.substT σ)) := by
  have hpw := reach_pointwise hσ hr
  have hnw : s = .neg → ∀ b, Caps.Sub (σ' b).reach (σ b).reach := fun hs b => by
    by_cases hb : b = .v i
    · subst hb; exact (hn hs).reach
    · rw [hσ b hb]; exact Caps.Sub.refl _
  have same : ∀ {t : Ty}, t.NoV i → LeLive S (t.substT σ') (t.substT σ) := fun h => by
    rw [h.substT_eq hσ]; exact .refl
  induction hf with
  | none h => exact ⟨fun _ => by rw [h.substT_eq hσ]; exact .refl, fun _ => same h⟩
  | var hq =>
    simp only [Ty.substT]
    rcases hq with ⟨rfl, hs⟩ | ⟨rfl, hs⟩
    · exact ⟨fun _ => hp hs, fun h => absurd h (by simp)⟩
    · exact ⟨fun h => absurd h (by simp), fun _ => hn hs⟩
  | @arrow q a φ b C hpa hPa hφ hPb hC iha ihb =>
    simp only [Ty.substT]
    rw [hφ.substT_eq hσ]
    refine ⟨fun hq => ?_, fun hq => ?_⟩
    · subst hq
      refine .arrow (iha.2 rfl) (Stage.use_mono (Stage.le_refl _)
        (Caps.substT_widen hpw)) (ihb.1 rfl) fun x hx => ?_
      rcases hx with hx | hx | hx
      · exact .inl (Ty.reach_substT_mono hpw x hx)
      · exact .inr (.inl (Ty.reach_substT_mono hpw x hx))
      · exact .inr (.inr (Caps.substT_widen hpw x hx))
    · subst hq
      by_cases hsn : s = .neg
      · have hw := hnw hsn
        refine .arrow (iha.1 rfl) (Stage.use_mono (Stage.le_refl _)
          (Caps.substT_widen hw)) (ihb.2 rfl) fun x hx => ?_
        rcases hx with hx | hx | hx
        · exact .inl (Ty.reach_substT_mono hw x hx)
        · exact .inr (.inl (Ty.reach_substT_mono hw x hx))
        · exact .inr (.inr (Caps.substT_widen hw x hx))
      · have hN := (Ty.Pol.arrow hpa hPa hφ hPb hC).noV (Variance.noV_cond hsn)
        have := same hN
        simpa only [Ty.substT, hφ.substT_eq hσ] using this
  | @eff q r e t C hr' he hPt hC iht =>
    simp only [Ty.substT]
    have er := hr'.substT_eq hσ
    have ee := he.substT_eq hσ
    simp only [Ty.substT, Ty.row.injEq] at er ee
    rw [er, ee]
    refine ⟨fun hq => ?_, fun hq => ?_⟩
    · subst hq
      refine .eff (Stage.use_mono (Stage.le_refl _) (Caps.substT_widen hpw))
        (iht.1 rfl) fun x hx => ?_
      rcases hx with hx | hx
      · exact .inl (Ty.reach_substT_mono hpw x hx)
      · exact .inr (Caps.substT_widen hpw x hx)
    · subst hq
      by_cases hsn : s = .neg
      · have hw := hnw hsn
        refine .eff (Stage.use_mono (Stage.le_refl _) (Caps.substT_widen hw))
          (iht.2 rfl) fun x hx => ?_
        rcases hx with hx | hx
        · exact .inl (Ty.reach_substT_mono hw x hx)
        · exact .inr (Caps.substT_widen hw x hx)
      · have hN := (Ty.Pol.eff hr' he hPt hC).noV (Variance.noV_cond hsn)
        have := same hN
        simpa only [Ty.substT, er, ee] using this
  | @data q t T us hPp hng hPn hin hbi ht ihp ihn =>
    subst ht
    rw [Ty.tcon_substT, Ty.tcon_substT]
    have get : ∀ {τ : Atom → Ty} {j : Nat} {x : Ty}, (us.map (Ty.substT τ))[j]? = some x →
        ∃ u, us[j]? = some u ∧ x = u.substT τ := fun h => by
      simp only [List.getElem?_map, Option.map_eq_some_iff] at h
      obtain ⟨u, hu, rfl⟩ := h
      exact ⟨u, hu, rfl⟩
    refine ⟨fun hq => ?_, fun hq => ?_⟩
    · subst hq
      refine .data (by simp) (fun j x x' h h' => ?_) (fun j x x' h h' _ hv => ?_)
        (fun j x x' h h' _ hv => ?_) (fun j x x' h h' _ hv => ?_) rfl rfl
      all_goals
        obtain ⟨u, hu, rfl⟩ := get h
        obtain ⟨u', hu', rfl⟩ := get h'
        rw [hu] at hu'; cases hu'
      · exact Ty.reach_substT_mono hpw
      · exact (ihp j u hu hv).1 rfl
      · exact (ihn j u hu hv).2 rfl
      · exact (hin j u hu hv).substT_eq hσ
    · subst hq
      by_cases hsn : s = .neg
      · have hw := hnw hsn
        refine .data (by simp) (fun j x x' h h' => ?_) (fun j x x' h h' _ hv => ?_)
          (fun j x x' h h' _ hv => ?_) (fun j x x' h h' _ hv => ?_) rfl rfl
        all_goals
          obtain ⟨u, hu, rfl⟩ := get h
          obtain ⟨u', hu', rfl⟩ := get h'
          rw [hu] at hu'; cases hu'
        · exact Ty.reach_substT_mono hw
        · exact (ihp j u hu hv).2 rfl
        · exact (ihn j u hu hv).1 rfl
        · exact ((hin j u hu hv).substT_eq hσ).symm
      · have hN := (Ty.Pol.data hPp hng hPn hin hbi rfl).noV (Variance.noV_cond hsn)
        have := same hN
        simpa only [Ty.tcon_substT] using this

/-- Substitutions that differ only at variables below `n`, each of which `t` does not hold
where they differ, agree on `t`. -/
theorem Ty.substT_eq_upto {t : Ty} {σ σ' : Atom → Ty} : ∀ n : Nat,
    (∀ a, (∀ j, j < n → a ≠ .v j) → σ a = σ' a) →
    (∀ j, j < n → σ (.v j) ≠ σ' (.v j) → t.NoV j) → t.substT σ = t.substT σ'
  | 0, hσ, _ => Ty.substT_congr fun a => hσ a fun j hj => absurd hj (Nat.not_lt_zero _)
  | n + 1, hσ, h => by
    let μ : Atom → Ty := fun a => if a = .v n then σ' a else σ a
    have e1 : t.substT σ = t.substT μ := by
      by_cases hn : σ (.v n) = σ' (.v n)
      · exact Ty.substT_congr fun a => by
          by_cases ha : a = .v n
          · subst ha; simp [μ, hn]
          · simp [μ, ha]
      · exact (h n (by omega) hn).substT_eq fun a ha => by simp [μ, ha]
    rw [e1]
    refine Ty.substT_eq_upto n (fun a ha => ?_) (fun j hj hne => h j (by omega) ?_)
    · by_cases han : a = .v n
      · subst han; simp [μ]
      · simp only [μ, han, if_false]
        exact hσ a fun j hj => by
          by_cases hjn : j = n
          · subst hjn; exact han
          · exact ha j (by omega)
    · have : (Atom.v j) ≠ .v n := fun e => by injection e; omega
      simpa [μ, this] using hne

/-- `us'` varies `us` as subtyping of data allows (regions.md §2.2.1, `LeLive.data`): each
argument keeps what it reaches, and one that is not the hidden row moves by the variance of
its parameter. -/
def Sig.Vary (S : Sig) (T : Name) (us us' : List Ty) : Prop :=
  us.length = us'.length ∧
  (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' → Caps.Sub u.reach u'.reach) ∧
  (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
    S.var T j = .pos → LeLive S u u') ∧
  (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
    S.var T j = .neg → LeLive S u' u) ∧
  (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
    S.var T j = .inv → u = u')

/-- Arguments of the same position. -/
theorem List.getElem?_pair {us us' : List Ty} {j : Nat} {u : Ty} (hl : us.length = us'.length)
    (h : us[j]? = some u) : ∃ u', us'[j]? = some u' :=
  ⟨_, List.getElem?_eq_getElem (hl ▸ (List.getElem?_eq_some_iff.mp h).1)⟩

/-- Equal positions vary as nothing. -/
theorem Sig.Vary.of_agree {T : Name} {us us' : List Ty} (hl : us.length = us'.length)
    (P : Nat → Prop) (hP : ∀ j, ¬ P j → us[j]? = us'[j]?)
    (hr : ∀ j u u', P j → us[j]? = some u → us'[j]? = some u' → Caps.Sub u.reach u'.reach)
    (hp : ∀ j u u', P j → us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
      S.var T j = .pos → LeLive S u u')
    (hn : ∀ j u u', P j → us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
      S.var T j = .neg → LeLive S u' u)
    (hi : ∀ j u u', P j → us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
      S.var T j = .inv → u = u') : S.Vary T us us' := by
  refine ⟨hl, fun j u u' h h' => ?_, fun j u u' h h' hh hv => ?_, fun j u u' h h' hh hv => ?_,
    fun j u u' h h' hh hv => ?_⟩
  all_goals by_cases hj : P j
  any_goals rw [hP j hj, h'] at h; cases h
  · exact hr j u u' hj h h'
  · exact Caps.Sub.refl _
  · exact hp j u u' hj h h' hh hv
  · exact .refl
  · exact hn j u u' hj h h' hh hv
  · exact .refl
  · exact hi j u u' hj h h' hh hv
  · rfl

theorem Sig.Vary.refl {T : Name} {us : List Ty} : S.Vary T us us :=
  .of_agree rfl (fun _ => False) (fun _ _ => rfl) (fun _ _ _ h => h.elim)
    (fun _ _ _ h => h.elim) (fun _ _ _ h => h.elim) (fun _ _ _ h => h.elim)

theorem Sig.Vary.length {T : Name} {us us' : List Ty} (h : S.Vary T us us') :
    us.length = us'.length := h.1

theorem Sig.Vary.nil {T : Name} {ws : List Ty} (h : S.Vary T [] ws) : ws = [] :=
  List.eq_nil_of_length_eq_zero h.length.symm

theorem Sig.Vary.trans {T : Name} {us vs ws : List Ty} (h₁ : S.Vary T us vs)
    (h₂ : S.Vary T vs ws) : S.Vary T us ws := by
  obtain ⟨l₁, r₁, p₁, n₁, i₁⟩ := h₁
  obtain ⟨l₂, r₂, p₂, n₂, i₂⟩ := h₂
  refine ⟨l₁.trans l₂, fun j u w h h' => ?_, fun j u w h h' hh hv => ?_,
    fun j u w h h' hh hv => ?_, fun j u w h h' hh hv => ?_⟩
  all_goals obtain ⟨v, hv'⟩ := List.getElem?_pair l₁ h
  · exact (r₁ j u v h hv').trans (r₂ j v w hv' h')
  · exact (p₁ j u v h hv' hh hv).trans (p₂ j v w hv' h' hh hv)
  · exact (n₂ j v w hv' h' hh hv).trans (n₁ j u v h hv' hh hv)
  · exact (i₁ j u v h hv' hh hv).trans (i₂ j v w hv' h' hh hv)

/-- Widening the hidden row is a variation. -/
theorem Sig.Vary.of_hide {T : Name} {i : Nat} {us us' : List Ty} (hi : S.hidden T = some i)
    (hl : us.length = us'.length) (heq : ∀ j, j ≠ i → us[j]? = us'[j]?)
    (hsub : ∀ r r', us[i]? = some r → us'[i]? = some r' → Caps.Sub r.reach r'.reach) :
    S.Vary T us us' :=
  .of_agree hl (· = i) heq (fun j u u' hj h h' => by subst hj; exact hsub u u' h h')
    (fun j _ _ hj _ _ hh => by subst hj; exact absurd hi hh)
    (fun j _ _ hj _ _ hh => by subst hj; exact absurd hi hh)
    (fun j _ _ hj _ _ hh => by subst hj; exact absurd hi hh)

/-- A parameter that may change: not invariant, or the hidden row. -/
def Sig.Varies (S : Sig) (T : Name) (j : Nat) : Prop := S.var T j ≠ .inv ∨ S.hidden T = some j

/-- A parameter that may change is an ordinary variable of every constructor, and nothing
else a constructor builds depends on it. -/
theorem Sig.varies_ok {T : Name} {j : Nat} (h : S.Varies T j) {K : Name} {k : Nat}
    {ts : List Ty} (hK : S.ctor K = some (k, ts, T)) :
    j < k ∧ (S.res K)[j]? = some (.tvar j) ∧
      ∀ m u, (S.res K)[m]? = some u → m ≠ j → u.NoV j := by
  rcases h with h | h
  · obtain ⟨h1, h2, h3, -⟩ := S.var_ok T j h K k ts hK; exact ⟨h1, h2, h3⟩
  · obtain ⟨h1, h2, h3, -⟩ := S.hidden_ok T j h K k ts hK; exact ⟨h1, h2, h3⟩

theorem Sig.Vary.eq_of_not_varies {T : Name} {us us' : List Ty} (hw : S.Vary T us us')
    {j : Nat} (hj : ¬ S.Varies T j) : us[j]? = us'[j]? := by
  have hinv : S.var T j = .inv := by
    exact Classical.byContradiction fun h => hj (.inl h)
  have hh : S.hidden T ≠ some j := fun h => hj (.inr h)
  cases hu : us[j]? with
  | none =>
    cases hu' : us'[j]? with
    | none => rfl
    | some u' =>
      obtain ⟨u, hu₀⟩ := List.getElem?_pair hw.length.symm hu'
      rw [hu] at hu₀; cases hu₀
  | some u =>
    obtain ⟨u', hu'⟩ := List.getElem?_pair hw.length hu
    rw [hu', hw.2.2.2.2 j u u' hu hu' hh hinv]

theorem Ty.tcon_up_substT (T : Name) (us : List Ty) (k : Nat) (ρ : Atom → Ty) :
    ((Ty.tcon T us).up k).substT ρ = Ty.tcon T (us.map fun u => (u.up k).substT ρ) := by
  simp only [Ty.up, Ty.substT_ren, Ty.tcon_substT]

/-- A constructor that builds data builds every variation of it: what may change is a
variable of what it builds, and nothing else there depends on it. -/
theorem Sig.builds_vary {K T : Name} {us us' : List Ty} (hw : S.Vary T us us')
    (h : S.Builds K (Ty.tcon T us)) : S.Builds K (Ty.tcon T us') := by
  classical
  intro k ts T₁ us₁ hK ht hlen
  obtain ⟨rfl, rfl⟩ := Ty.tcon_eq ht
  obtain ⟨ρ, hρ⟩ := h k ts T us hK rfl (hw.length.trans hlen)
  let ρ' : Atom → Ty := fun a => match a with
    | .v j => if hj : S.Varies T j ∧ j < us'.length then ((us'[j]'hj.2).up k).substT ρ
        else ρ (.v j)
    | .r p => ρ (.r p)
  have hhi : ∀ j, k ≤ j → ρ' (.v j) = ρ (.v j) := fun j hj => by
    simp only [ρ']
    split
    · rename_i h; exact absurd (Sig.varies_ok h.1 hK).1 (by omega)
    · rfl
  have hup : ∀ u : Ty, (u.up k).substT ρ' = (u.up k).substT ρ := fun u => by
    simp only [Ty.up, Ty.substT_ren]
    apply Ty.substT_congr
    intro a
    cases ha : a.up k with
    | v j =>
      have : k ≤ j := by
        exact Classical.byContradiction fun hj => Atom.up_ne (a := a) (by omega : j < k) ha
      exact hhi j this
    | r p => rfl
  refine ⟨ρ', ?_⟩
  rw [Ty.tcon_up_substT] at hρ ⊢
  rw [Ty.tcon_substT] at hρ ⊢
  obtain ⟨-, hρl⟩ := Ty.tcon_eq hρ
  simp only [hup]
  congr 1
  apply List.ext_getElem?
  intro n
  simp only [List.getElem?_map]
  by_cases hn : S.Varies T n
  · obtain ⟨-, hrn, -⟩ := Sig.varies_ok hn hK
    have hnl : n < us'.length := by rw [hlen]; exact (List.getElem?_eq_some_iff.mp hrn).1
    rw [hrn, List.getElem?_eq_getElem hnl]
    simp [Ty.substT, ρ', hn, hnl]
  · have e := congrArg (·[n]?) hρl
    simp only [List.getElem?_map] at e
    rw [← hw.eq_of_not_varies hn]
    cases hrn : (S.res K)[n]? with
    | none => rw [hrn] at e; rw [← e]; rfl
    | some u =>
      rw [hrn] at e
      rw [← e]
      simp only [Option.map_some, Option.some.injEq]
      symm
      refine Ty.substT_eq_upto k (fun a ha => ?_) (fun j hj hne => ?_)
      · cases a with
        | v j => exact (hhi j (Classical.byContradiction fun h => ha j (by omega) rfl)).symm
        | r p => rfl
      · have hvj : S.Varies T j := Classical.byContradiction fun hv => hne (by simp [ρ', hv])
        exact (Sig.varies_ok hvj hK).2.2 n u hrn fun e => hn (e ▸ hvj)

end Flick
