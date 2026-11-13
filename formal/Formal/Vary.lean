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
  | rcons _ _ iht ihr => exact Ty.NoV.rcons (iht hs) (ihr hs)
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
  | rcons _ _ iht ihr =>
    exact ⟨fun hq => .rcons' (iht.1 hq) (ihr.1 hq), fun hq => .rcons' (iht.2 hq) (ihr.2 hq)⟩
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
      · rw [(hin j u hu hv).substT_eq hσ]; exact .refl
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
        · rw [(hin j u hu hv).substT_eq hσ]; exact .refl
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

/-- Ignored variables may differ syntactically or by row congruence. -/
theorem Ty.substT_equiv_upto {t : Ty} {σ τ : Atom → Ty} (n : Nat)
    (hσ : ∀ a, (∀ j, j < n → a ≠ .v j) → Ty.Equiv (σ a) (τ a))
    (h : ∀ j, j < n → ¬ Ty.Equiv (σ (.v j)) (τ (.v j)) → t.NoV j) :
    Ty.Equiv (t.substT σ) (t.substT τ) := by
  classical
  let μ : Atom → Ty := fun a => if Ty.Equiv (σ a) (τ a) then τ a else σ a
  have e1 : Ty.Equiv (t.substT σ) (t.substT μ) := t.substT_equiv fun a => by
    by_cases ha : Ty.Equiv (σ a) (τ a)
    · simp only [μ, ha, ↓reduceIte]
    · simp only [μ, ha, ↓reduceIte]; exact .refl
  have e2 : t.substT μ = t.substT τ := Ty.substT_eq_upto n
    (fun a ha => by simp only [μ, hσ a ha, ↓reduceIte])
    (fun j hj hne => h j hj (fun he => hne (by simp only [μ, he, ↓reduceIte])))
  rw [e2] at e1
  exact e1

end Flick
