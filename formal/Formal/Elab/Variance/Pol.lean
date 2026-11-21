import Formal.Elab.Variance.Order

/-!
# Polarity (`Ty.Pol`, `Cn.Pol`) and `Prog.ClsVarOk` (`Formal.Elab.Variance.Pol`)
-/

namespace Flick.Elab

/-! ### Polarity (`Ty.Pol`, `Cn.Pol`) and `Prog.ClsVarOk` -/

/-- Polarity of class parameter `i` (with variance `s`) in a type `t` at polarity `q`
(`true`: covariant). Matches `Flick.Ty.Pol` in the core: a covariant parameter never stands
under a contravariant flip (where it would grow reach while subtyping asks it to shrink). -/
inductive Ty.Pol (dvar : Nat → Nat → Variance) (i : Nat) (s : Variance) : Bool → Ty → Prop
  | none {q t} : ¬ t.Occurs i → Ty.Pol dvar i s q t
  | var {q} : s.at q → Ty.Pol dvar i s q (.var i)
  | arr {q φ a b c} :
      ¬ φ.Occurs i →
      (s = .pos → ¬ a.Occurs i) →
      Ty.Pol dvar i s (!q) a →
      Ty.Pol dvar i s q b →
      (¬ c.Occurs i ∨ s.at q) →
      Ty.Pol dvar i s q (.arr φ a b c)
  | data {q t T us} :
      T ≠ 2 → T ≠ 4 →
      (∀ j u, us[j]? = some u → dvar T j = .pos → Ty.Pol dvar i s q u) →
      (∀ j u, us[j]? = some u → dvar T j = .neg → s = .pos → ¬ u.Occurs i) →
      (∀ j u, us[j]? = some u → dvar T j = .neg → Ty.Pol dvar i s (!q) u) →
      (∀ j u, us[j]? = some u → dvar T j = .inv → ¬ u.Occurs i) →
      (∀ j u, us[j]? = some u → dvar T j = .bi → ¬ s.at q → ¬ u.Occurs i) →
      t = Ty.tcon T us → Ty.Pol dvar i s q t

/-- Polarity of class parameter `i` (with variance `s`) in a constraint `c` at polarity `q`
(`true` for a superclass slot of a dictionary, `false` for a method's own constraint). -/
def Cn.Pol (dvar cvar : Nat → Nat → Variance) (i : Nat) (s : Variance) (q : Bool) (c : Cn) : Prop :=
  (∀ j u, c.args[j]? = some u → cvar c.cls j = .pos → Ty.Pol dvar i s q u) ∧
  (∀ j u, c.args[j]? = some u → cvar c.cls j = .neg → s = .pos → ¬ u.Occurs i) ∧
  (∀ j u, c.args[j]? = some u → cvar c.cls j = .neg → Ty.Pol dvar i s (!q) u) ∧
  (∀ j u, c.args[j]? = some u → (cvar c.cls j = .inv ∨ cvar c.cls j = .bi) → ¬ u.Occurs i)

theorem Ty.Pol.noOccurs {dvar : Nat → Nat → Variance} {i : Nat} {s : Variance} {q : Bool} {t : Ty}
    (h : Ty.Pol dvar i s q t) (hs : (¬ s.at q ∧ ¬ s.at (!q)) ∨ (s = .pos ∧ q = false)) :
    ¬ t.Occurs i := by
  induction h with
  | none hn => exact hn
  | var hq =>
    rcases hs with ⟨h1, -⟩ | ⟨rfl, rfl⟩
    · exact absurd hq h1
    · simp [Variance.at] at hq
  | arr hφ hpa _ _ hc iha ihb =>
    rintro (ha | hb | hc' | hφ')
    · rcases hs with ⟨h1, h2⟩ | ⟨rfl, rfl⟩
      · exact iha (.inl ⟨h2, by simpa using h1⟩) ha
      · exact hpa rfl ha
    · exact ihb hs hb
    · rcases hc with hc | hc
      · exact hc hc'
      · rcases hs with ⟨h1, -⟩ | ⟨rfl, rfl⟩
        · exact absurd hc h1
        · simp [Variance.at] at hc
    · exact hφ hφ'
  | data _ _ _ hng _ hin hbi ht ihp ihn =>
    subst ht
    intro hocc
    obtain ⟨j, u, hu, hju⟩ := Ty.occurs_tcon_iff.mp hocc
    cases hv : dvar _ j with
    | pos => exact ihp j u hu hv hs hju
    | neg =>
      rcases hs with ⟨h1, h2⟩ | ⟨rfl, rfl⟩
      · exact ihn j u hu hv (.inl ⟨h2, by simpa using h1⟩) hju
      · exact hng j u hu hv rfl hju
    | inv => exact hin j u hu hv hju
    | bi =>
      rcases hs with ⟨h1, -⟩ | ⟨rfl, rfl⟩
      · exact hbi j u hu hv h1 hju
      · exact hbi j u hu hv (by simp [Variance.at]) hju

theorem Variance.noOccurs_cond {s : Variance} (h : s ≠ .neg) :
    (¬ s.at false ∧ ¬ s.at (!false)) ∨ (s = .pos ∧ false = false) := by
  cases s with
  | pos => exact .inr ⟨rfl, rfl⟩
  | neg => exact absurd rfl h
  | inv | bi => exact .inl ⟨by simp [Variance.at], by simp [Variance.at]⟩

private theorem ty_subst_agree {t : Ty} {σ σ' : Nat → Ty} (h : ∀ n, t.Occurs n → σ n = σ' n) :
    t.subst σ = t.subst σ' := by
  induction t using Ty.ind generalizing σ σ' with
  | all k t iht =>
    simp only [Ty.subst]
    rw [iht fun n hn => ?_]
    simp only [liftTy]
    split
    · rfl
    · rw [h (n - k) (by rwa [Ty.Occurs, Nat.sub_add_cancel (by omega)])]
  | var n => exact h n rfl
  | con _ | cnil => rfl
  | app _ _ iht ihu | cun _ _ iht ihu | run _ _ iht ihu =>
    simp only [Ty.subst]
    rw [iht fun n hn => h n (.inl hn), ihu fun n hn => h n (.inr hn)]
  | rnm _ _ iht => simp only [Ty.subst]; rw [iht h]
  | arr φ _ _ _ iha ihb ihq ihφ =>
    simp only [Ty.subst]
    rw [iha fun n hn => h n (.inl hn), ihb fun n hn => h n (.inr (.inl hn)),
      ihq fun n hn => h n (.inr (.inr (.inl hn)))]
    cases φ with
    | P => rfl
    | R _ _ =>
      simp only [Stage.subst]
      rw [ihφ.1 fun n hn => h n (.inr (.inr (.inr (.inl hn)))),
        ihφ.2 fun n hn => h n (.inr (.inr (.inr (.inr hn))))]

private theorem stage_subst_agree {φ : Stage} {σ σ' : Nat → Ty}
    (h : ∀ n, φ.Occurs n → σ n = σ' n) : φ.subst σ = φ.subst σ' := by
  cases φ with
  | P => rfl
  | R r e =>
    simp only [Stage.subst]
    rw [ty_subst_agree fun n hn => h n (.inl hn), ty_subst_agree fun n hn => h n (.inr hn)]

private theorem shown_subst_mono {t : Ty} {σ σ' : Nat → Ty}
    (h : ∀ k n, (σ k).Shown n → (σ' k).Shown n) :
    ∀ n, (t.subst σ).Shown n → (t.subst σ').Shown n := fun n hn => by
  obtain ⟨m, hm, hσ⟩ := (Ty.shown_subst_iff σ t n).1 hn
  exact (Ty.shown_subst_iff σ' t n).2 ⟨m, hm, h m n hσ⟩

/-- Varying one parameter `i` in a type whose polarity obeys `Ty.Pol` moves the substituted type
in the direction of `s` and `q`. -/
theorem Ty.Pol.vary {dvar : Nat → Nat → Variance} {i : Nat} {s : Variance} {q : Bool} {t : Ty}
    (ht : Ty.Pol dvar i s q t) {σ σ' : Nat → Ty} (hσ : ∀ n, n ≠ i → σ n = σ' n)
    (hr : ∀ n, (σ i).Shown n → (σ' i).Shown n)
    (hp : s = .pos → Ty.Le dvar (σ i) (σ' i))
    (hn : s = .neg → Ty.Le dvar (σ' i) (σ i)) :
    (q = true → Ty.Le dvar (t.subst σ) (t.subst σ')) ∧
    (q = false → Ty.Le dvar (t.subst σ') (t.subst σ)) := by
  have hpw : ∀ k n, (σ k).Shown n → (σ' k).Shown n := fun k n hk => by
    by_cases hki : k = i
    · subst hki; exact hr n hk
    · rw [← hσ k hki]; exact hk
  have hnw : s = .neg → ∀ k n, (σ' k).Shown n → (σ k).Shown n := fun hs k n hk => by
    by_cases hki : k = i
    · subst hki; exact (hn hs).shown n hk
    · rw [hσ k hki]; exact hk
  have same : ∀ {u : Ty}, ¬ u.Occurs i → u.subst σ = u.subst σ' := fun hu =>
    ty_subst_agree fun n hno => hσ n fun e => hu (e ▸ hno)
  have use_mono : ∀ {φ : Stage} {c : Ty} {τ₁ τ₂ : Nat → Ty},
      φ.subst τ₁ = φ.subst τ₂ → (∀ k n, (τ₁ k).Shown n → (τ₂ k).Shown n) →
      Stage.UseLe (φ.subst τ₁) (c.subst τ₁) (φ.subst τ₂) (c.subst τ₂) := fun {φ c τ₁ τ₂} hφ hτ => by
    rw [← hφ]
    cases φ.subst τ₁ with
    | P => trivial
    | R r e => exact ⟨.refl, rfl, fun n hn => .inl (shown_subst_mono hτ n hn)⟩
  induction ht with
  | none hno =>
    have he := same hno
    exact ⟨fun _ => by rw [he]; exact .refl, fun _ => by rw [he]; exact .refl⟩
  | var hq =>
    simp only [Ty.subst]
    rcases hq with ⟨rfl, hs⟩ | ⟨rfl, hs⟩
    · exact ⟨fun _ => hp hs, fun h => absurd h (by simp)⟩
    · exact ⟨fun h => absurd h (by simp), fun _ => hn hs⟩
  | @arr q φ a b c hφ hpa hPa hPb hc iha ihb =>
    have hφe : φ.subst σ = φ.subst σ' := stage_subst_agree fun n hno => hσ n fun e => hφ (e ▸ hno)
    refine ⟨fun hq => ?_, fun hq => ?_⟩
    · subst hq
      exact .arr (iha.2 rfl) (use_mono hφe hpw) (ihb.1 rfl)
        (shown_subst_mono (t := .arr φ a b c) hpw)
    · subst hq
      by_cases hsn : s = .neg
      · exact .arr (iha.1 rfl) (use_mono hφe.symm (hnw hsn)) (ihb.2 rfl)
          (shown_subst_mono (t := .arr φ a b c) (hnw hsn))
      · have hno := (Ty.Pol.arr hφ hpa hPa hPb hc).noOccurs (Variance.noOccurs_cond hsn)
        rw [same hno]; exact .refl
  | @data q t T us h2 h4 hPp hng hPn hin hbi ht ihp ihn =>
    subst ht
    rw [Ty.tcon_subst, Ty.tcon_subst]
    refine ⟨fun hq => ?_, fun hq => ?_⟩
    · subst hq
      refine .data h2 h4 (by simp) (fun j x y hx hy => ?_) (fun j x y hx hy hv => ?_)
        (fun j x y hx hy hv => ?_) (fun j x y hx hy hv => ?_)
      all_goals
        simp only [List.getElem?_map, Option.map_eq_some_iff] at hx hy
        obtain ⟨u, hu, rfl⟩ := hx
        obtain ⟨u', hu', rfl⟩ := hy
        rw [hu] at hu'; cases hu'
      · exact shown_subst_mono hpw
      · exact (ihp j u hu hv).1 rfl
      · exact (ihn j u hu hv).2 rfl
      · exact same (hin j u hu hv)
    · subst hq
      by_cases hsn : s = .neg
      · refine .data h2 h4 (by simp) (fun j x y hx hy => ?_) (fun j x y hx hy hv => ?_)
          (fun j x y hx hy hv => ?_) (fun j x y hx hy hv => ?_)
        all_goals
          simp only [List.getElem?_map, Option.map_eq_some_iff] at hx hy
          obtain ⟨u, hu, rfl⟩ := hx
          obtain ⟨u', hu', rfl⟩ := hy
          rw [hu] at hu'; cases hu'
        · exact shown_subst_mono (hnw hsn)
        · exact (ihp j u hu hv).2 rfl
        · exact (ihn j u hu hv).1 rfl
        · exact (same (hin j u hu hv)).symm
      · have hno := (Ty.Pol.data h2 h4 hPp hng hPn hin hbi rfl).noOccurs (Variance.noOccurs_cond hsn)
        have he := same hno
        rw [Ty.tcon_subst, Ty.tcon_subst] at he
        rw [he]; exact .refl

/-- Varying one parameter `i` in a constraint obeying `Cn.Pol ... true` moves the constraint in
the direction of `Cn.Serves`. -/
theorem Cn.Pol.vary {P : Prog} {i : Nat} {s : Variance} {c : Cn}
    (hc : Cn.Pol P.dataVar P.clsVar i s true c) {σ σ' : Nat → Ty} (hσ : ∀ n, n ≠ i → σ n = σ' n)
    (hr : ∀ n, (σ i).Shown n → (σ' i).Shown n)
    (hp : s = .pos → Ty.Le P.dataVar (σ i) (σ' i))
    (hn : s = .neg → Ty.Le P.dataVar (σ' i) (σ i)) :
    Cn.Serves P (c.subst σ') (c.subst σ) := by
  have hpw : ∀ k n, (σ k).Shown n → (σ' k).Shown n := fun k n hk => by
    by_cases hki : k = i
    · subst hki; exact hr n hk
    · rw [← hσ k hki]; exact hk
  refine ⟨rfl, by simp [Cn.subst], fun j x y hx hy hv => ?_,
    fun j x y hx hy hv => ?_, fun j x y hx hy hv => ?_⟩
  all_goals
    simp only [Cn.subst, List.getElem?_map, Option.map_eq_some_iff] at hx hy
    obtain ⟨u, hu, rfl⟩ := hx
    obtain ⟨u', hu', rfl⟩ := hy
    rw [hu] at hu'; cases hu'
  · exact ((hc.1 j u hu hv).vary hσ hr hp hn).1 rfl
  · exact ⟨((hc.2.2.1 j u hu hv).vary hσ hr hp hn).2 rfl, shown_subst_mono hpw⟩
  · exact (ty_subst_agree fun n hno => hσ n fun e => hc.2.2.2 j u hu hv (e ▸ hno)).symm

/-- Soundness condition for class parameter variances in a program `P`: every superclass of `C`
respects `P.clsVar C i`, and every method of `C` respects `P.clsVar C i` in its result type and
extra method constraints. -/
structure Prog.ClsVarOk (P : Prog) : Prop where
  supers : ∀ C i, P.clsVar C i ≠ .inv → ∀ S ∈ P.supers C,
    (P.clsVar C i = .bi → ∀ t ∈ S.args, ¬ t.Occurs i) ∧
    (P.clsVar C i = .pos ∨ P.clsVar C i = .neg → Cn.Pol P.dataVar P.clsVar i (P.clsVar C i) true S)
  methods : ∀ m C s c₀ rest i, P.methods m = some (C, s) → s.ctx = c₀ :: rest →
    P.clsVar C i ≠ .inv →
    (P.clsVar C i = .bi → ¬ s.ty.Occurs i ∧ ∀ c ∈ rest, ∀ t ∈ c.args, ¬ t.Occurs i) ∧
    (P.clsVar C i = .pos ∨ P.clsVar C i = .neg →
      Ty.Pol P.dataVar i (P.clsVar C i) true s.ty ∧
      ∀ c ∈ rest, Cn.Pol P.dataVar P.clsVar i (P.clsVar C i) false c)

private def paramsMix (hs qs : List Ty) (m : Nat) : Nat → Ty :=
  fun j => if j < m then params qs j else params hs j

private theorem paramsMix_zero (hs qs : List Ty) : paramsMix hs qs 0 = params hs := by
  funext j; simp [paramsMix]

private theorem paramsMix_end {hs qs : List Ty} (hl : qs.length = hs.length) :
    paramsMix hs qs hs.length = params qs := by
  funext j
  simp only [paramsMix]
  by_cases hj : j < hs.length
  · simp [hj]
  · simp only [hj, ↓reduceIte, params, List.getD_eq_getElem?_getD]
    rw [List.getElem?_eq_none (by omega), List.getElem?_eq_none (by omega)]

/-- `Prog.ClsVarOk` guarantees that `Cn.Serves` is preserved by superclass projection:
if `H` serves `Q`, then the `i`-th superclass of `H` serves the `i`-th superclass of `Q`. -/
theorem Prog.ClsVarOk.super_serves {P : Prog} (hok : P.ClsVarOk) {Q H : Cn} {i : Nat}
    {SQ SH : Cn} (hserv : Cn.Serves P Q H) (hQ : P.super Q i = some SQ)
    (hH : P.super H i = some SH) : Cn.Serves P SQ SH := by
  obtain ⟨hc, hl, hp, hn, hi⟩ := hserv
  unfold Prog.super at hQ hH
  rw [hc] at hH
  cases hS₀ : (P.supers Q.cls)[i]? with
  | none => rw [hS₀] at hQ; cases hQ
  | some S₀ =>
    rw [hS₀] at hQ hH
    simp only [Option.map_some, Option.some.injEq] at hQ hH
    subst hQ; subst hH
    have hmem : S₀ ∈ P.supers Q.cls := List.mem_of_getElem? hS₀
    have step : ∀ m, m < H.args.length →
        Cn.Serves P (S₀.subst (paramsMix H.args Q.args (m + 1)))
                    (S₀.subst (paramsMix H.args Q.args m)) := by
      intro m hm
      have hmQ : m < Q.args.length := hl.symm ▸ hm
      have gQ : Q.args[m]? = some Q.args[m] := List.getElem?_eq_getElem hmQ
      have gH : H.args[m]? = some H.args[m] := List.getElem?_eq_getElem hm
      have hσ : ∀ n, n ≠ m → paramsMix H.args Q.args m n = paramsMix H.args Q.args (m + 1) n := by
        intro n hnm
        simp only [paramsMix]
        by_cases h1 : n < m
        · simp [h1, show n < m + 1 by omega]
        · simp [h1, show ¬ n < m + 1 by omega]
      have ev : paramsMix H.args Q.args m m = H.args[m] := by
        simp [paramsMix, params, List.getD_eq_getElem?_getD, gH]
      have ev' : paramsMix H.args Q.args (m + 1) m = Q.args[m] := by
        simp [paramsMix, params, List.getD_eq_getElem?_getD, gQ]
      cases hv : P.clsVar Q.cls m with
      | pos =>
        have hle := hp m _ _ gQ gH hv
        have hpol := (hok.supers Q.cls m (by rw [hv]; decide) S₀ hmem).2 (.inl hv)
        rw [hv] at hpol
        exact hpol.vary hσ (by rw [ev, ev']; exact hle.shown)
          (fun _ => by rw [ev, ev']; exact hle) (fun h => by cases h)
      | neg =>
        obtain ⟨hle, hsh⟩ := hn m _ _ gQ gH hv
        have hpol := (hok.supers Q.cls m (by rw [hv]; decide) S₀ hmem).2 (.inr hv)
        rw [hv] at hpol
        exact hpol.vary hσ (by rw [ev, ev']; exact hsh)
          (fun h => by cases h) (fun _ => by rw [ev, ev']; exact hle)
      | inv =>
        have heq := hi m _ _ gQ gH (.inl hv)
        have hfun : paramsMix H.args Q.args (m + 1) = paramsMix H.args Q.args m := by
          funext n
          by_cases hnm : n = m
          · subst hnm; rw [ev, ev', heq]
          · exact (hσ n hnm).symm
        rw [hfun]; exact Cn.Serves.refl _
      | bi =>
        have heq := hi m _ _ gQ gH (.inr hv)
        have hfun : paramsMix H.args Q.args (m + 1) = paramsMix H.args Q.args m := by
          funext n
          by_cases hnm : n = m
          · subst hnm; rw [ev, ev', heq]
          · exact (hσ n hnm).symm
        rw [hfun]; exact Cn.Serves.refl _
    have chain : ∀ m, m ≤ H.args.length →
        Cn.Serves P (S₀.subst (paramsMix H.args Q.args m))
                    (S₀.subst (paramsMix H.args Q.args 0)) := by
      intro m; induction m with
      | zero => intro _; exact Cn.Serves.refl _
      | succ m ih => intro hm; exact (step m (by omega)).trans (ih (by omega))
    rw [← paramsMix_zero H.args Q.args, ← paramsMix_end hl]
    exact chain H.args.length (Nat.le_refl _)

end Flick.Elab
