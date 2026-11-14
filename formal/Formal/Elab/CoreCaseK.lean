import Formal.Elab.CoreCase
import Formal.ContextSubstitution

/-!
# λFlick⁺ as a `Target`: the law of an arm, assembled

`armτ` sends the core context of the open arm (`canonC`) to the arm of the core: its
dictionaries and fields to the variables of the pattern, the premises `Δl` and the variables
of `Γ` to theirs past the pattern, the closed premises to their dictionaries. Every unifier of
the closed problem types the body there (`has_caseK`).
-/

namespace Flick.Elab

variable {P : Prog} {S : Sig} {N : Names}

theorem app_get {α : Type} {A B : List α} {i : Nat} {x : α} (h : (A ++ B)[i]? = some x) :
    (i < A.length ∧ A[i]? = some x) ∨ (A.length ≤ i ∧ B[i - A.length]? = some x) := by
  by_cases hi : i < A.length
  · exact .inl ⟨hi, by rwa [List.getElem?_append_left hi] at h⟩
  · exact .inr ⟨by omega, by rwa [List.getElem?_append_right (by omega)] at h⟩

theorem map_get {α β : Type} {f : α → β} {A : List α} {i : Nat} {y : β} (h : (A.map f)[i]? = some y) :
    ∃ x, A[i]? = some x ∧ y = f x := by
  rw [List.getElem?_map] at h
  obtain ⟨x, hx, rfl⟩ := Option.map_eq_some_iff.mp h
  exact ⟨x, hx, rfl⟩

theorem lt_of_some {α : Type} {l : List α} {i : Nat} {x : α} (h : l[i]? = some x) : i < l.length :=
  (List.getElem?_eq_some_iff.mp h).1

theorem canonC_get {Γ : List TSch} {Δ : List Cn} {i : Nat} {s : Flick.Scheme}
    (h : (canonC (N := N) Γ Δ)[i]? = some s) :
    (i < Δ.length ∧ ∃ q, Δ[i]? = some q ∧ s = Flick.Scheme.mono (trD N q)) ∨
      (Δ.length ≤ i ∧ ∃ u, Γ[i - Δ.length]? = some u ∧ s = trS N u) := by
  unfold canonC at h
  rcases app_get h with ⟨hi, h⟩ | ⟨hi, h⟩
  · obtain ⟨q, hq, rfl⟩ := map_get h
    exact .inl ⟨by simpa using hi, q, hq, rfl⟩
  · obtain ⟨u, hu, rfl⟩ := map_get h
    exact .inr ⟨by simpa using hi, u, by simpa using hu, rfl⟩

variable (N) in
/-- The substitution of the closed body: `m` dictionaries and `n` fields, `L` premises `Δl`,
`D` premises in all and `G` variables in the open arm. -/
def armτ (m n L D G : Nat) (γ δ : Nat → Nat) (ρ : Nat → Dict) (j : Nat) : Flick.Tm :=
  if j < D + G then
    if j < m then .var j
    else if j < m + L then .var (δ (j - m) + (m + n))
    else if j < D then trDt N (fun _ => 0) (ρ (j - (m + L)))
    else if j < D + n then .var (m + (j - D))
    else .var (γ (j - D - n) + (m + n))
  else if (j - (D + G)) % 2 = 0 then .var (aγ m n γ ((j - (D + G)) / 2))
  else trDt N (aδ m n δ) (dsub (m + L) ρ ((j - (D + G)) / 2))

theorem armτ_γ {m n L D G : Nat} {γ δ : Nat → Nat} {ρ : Nat → Dict} (hD : m + L ≤ D) (hG : n ≤ G)
    {Gl : List TSch} {Dl : List Cn} (hGl : Gl.length = G) (hDl : Dl.length = D) (x : Nat) :
    armτ N m n L D G γ δ ρ (cγ Gl Dl x) = .var (aγ m n γ x) := by
  subst hGl hDl
  unfold cγ armτ aγ
  by_cases hx : x < Gl.length
  · simp only [hx, if_true, show Dl.length + x < Dl.length + Gl.length by omega,
      show ¬ Dl.length + x < m by omega, show ¬ Dl.length + x < m + L by omega,
      show ¬ Dl.length + x < Dl.length by omega, if_false]
    by_cases hn : x < n
    · simp [hn, show Dl.length + x < Dl.length + n by omega]
    · simp only [hn, show ¬ Dl.length + x < Dl.length + n by omega, if_false,
        show Dl.length + x - Dl.length - n = x - n by omega]
  · simp only [hx, if_false, show ¬ Dl.length + Gl.length + 2 * x < Dl.length + Gl.length by omega]
    rw [show Dl.length + Gl.length + 2 * x - (Dl.length + Gl.length) = 2 * x by omega]
    simp

theorem armτ_δ {m n L D G : Nat} {γ δ : Nat → Nat} {ρ : Nat → Dict} (hcl : ∀ k, (ρ k).Below 0)
    (hD : m + L ≤ D) {Gl : List TSch} {Dl : List Cn} (hGl : Gl.length = G) (hDl : Dl.length = D)
    (k : Nat) : armτ N m n L D G γ δ ρ (cδ Gl Dl k) = trDt N (aδ m n δ) (dsub (m + L) ρ k) := by
  subst hGl hDl
  unfold cδ armτ
  by_cases hk : k < Dl.length
  · simp only [hk, if_true, show k < Dl.length + Gl.length by omega]
    by_cases hm : k < m
    · simp [hm, dsub, show k < m + L by omega, trDt, aδ]
    · by_cases hL : k < m + L
      · simp [hm, hL, dsub, trDt, aδ]
      · simp only [hm, hL, if_false, dsub]
        exact trDt_below (hcl _)
  · simp only [hk, if_false, show ¬ Dl.length + Gl.length + 2 * k + 1 < Dl.length + Gl.length by omega]
    rw [show Dl.length + Gl.length + 2 * k + 1 - (Dl.length + Gl.length) = 2 * k + 1 by omega]
    simp only [show (2 * k + 1) % 2 ≠ 0 by omega, if_false, show (2 * k + 1) / 2 = k by omega]

theorem PatsTy.vars : ∀ (xs : List Flick.Ty), PatsTy S (List.replicate xs.length .var) xs xs
  | [] => .nil
  | x :: xs => PatsTy.cons (Δ₁ := [x]) .var (PatsTy.vars xs)

theorem has_caseK (hR : Real P S N) {Γ : List TSch} {Δl Δ : List Cn} {e b r : Tm} {K n T : Nat}
    {us : List Ty} {t : Ty} {φ : Stage} {c : Ctor} {θ σ : Nat → Ty} {ρ : Nat → Dict}
    (he : Has S N (Γ.map (·.subst σ)) (Δl.map (·.subst σ)) e (.mono ((Ty.tcon T us).subst σ))
      (φ.subst σ))
    (hK : P.ctors K = some c) (hT : c.T = T) (hlen : us.length = c.res.length)
    (hn : n = c.fields.length) (hU : Unify (c.eqs us) θ)
    (hb : Has S N (armT c θ Γ) (armΔ c θ (Δl ++ Δ)) b (.mono ((t.subst (up c.k)).subst θ))
      ((φ.subst (up c.k)).subst θ))
    (hρ : Closes P Δ σ ρ) (hcl : ∀ k, (ρ k).Below 0)
    (hr : Has S N (Γ.map (·.subst σ)) (Δl.map (·.subst σ)) r (.mono (t.subst σ)) (φ.subst σ)) :
    Has S N (Γ.map (·.subst σ)) (Δl.map (·.subst σ))
      (.caseK e K n c.ctx.length (b.dsubst ρ (c.ctx.length + Δl.length)) r) (.mono (t.subst σ))
      (φ.subst σ) := by
  subst hT hn
  intro Ψ Θ Γc γ δ hc
  obtain ⟨e', he', te⟩ := he Ψ Θ Γc γ δ hc
  obtain ⟨r', hr', tr⟩ := hr Ψ Θ Γc γ δ hc
  obtain ⟨hS, hres, -, -⟩ := hR.ctor hK
  obtain ⟨bu, hbu, tbu⟩ := hb [] Facts.none _ _ _
    (Corr.canon (N := N) (armT c θ Γ) (armΔ c θ (Δl ++ Δ)))
  have hD : (armΔ c θ (Δl ++ Δ)).length = c.ctx.length + Δl.length + Δ.length := by
    simp [armΔ]; omega
  have hG : (armT c θ Γ).length = c.fields.length + Γ.length := by simp [armT]
  have hbc := Tr.dsubst hcl hbu
    (τ := armτ N c.ctx.length c.fields.length Δl.length (armΔ c θ (Δl ++ Δ)).length
      (armT c θ Γ).length γ δ ρ) (γ₂ := aγ c.ctx.length c.fields.length γ)
    (δ₂ := aδ c.ctx.length c.fields.length δ) (L := c.ctx.length + Δl.length)
    (armτ_γ (by omega) (by omega) rfl rfl) (armτ_δ hcl (by omega) rfl rfl)
  have hfr : ∀ i, (bu.subst (armτ N c.ctx.length c.fields.length Δl.length
      (armΔ c θ (Δl ++ Δ)).length (armT c θ Γ).length γ δ ρ)).Free i →
      i < (Pat.spine (N.ctor K) (List.replicate (c.ctx.length + c.fields.length) .var)).binds +
        Γc.length := by
    intro i hi
    rw [spine_vars_binds]
    obtain ⟨j, hj, hτ⟩ := Flick.Tm.free_subst bu _ i hi
    have hjl := tbu.frees j hj
    simp only [canonC, List.length_append, List.length_map] at hjl
    unfold armτ at hτ
    rw [if_pos (by omega)] at hτ
    by_cases h₁ : j < c.ctx.length
    · rw [if_pos h₁] at hτ; simp only [Flick.Tm.Free] at hτ; omega
    · rw [if_neg h₁] at hτ
      by_cases h₂ : j < c.ctx.length + Δl.length
      · rw [if_pos h₂] at hτ; simp only [Flick.Tm.Free] at hτ
        have := lt_of_some (l := Γc) (i := δ (j - c.ctx.length)) (by
          rw [hc.dict, List.getElem?_map, List.getElem?_eq_getElem (by omega)]; rfl)
        omega
      · rw [if_neg h₂] at hτ
        by_cases h₃ : j < (armΔ c θ (Δl ++ Δ)).length
        · rw [if_pos h₃] at hτ; exact absurd hτ (trDt_noFree (hcl _) i)
        · rw [if_neg h₃] at hτ
          by_cases h₄ : j < (armΔ c θ (Δl ++ Δ)).length + c.fields.length
          · rw [if_pos h₄] at hτ; simp only [Flick.Tm.Free] at hτ; omega
          · rw [if_neg h₄] at hτ; simp only [Flick.Tm.Free] at hτ
            have := lt_of_some (l := Γc) (i := γ (j - (armΔ c θ (Δl ++ Δ)).length - c.fields.length))
              (by rw [hc.var, List.getElem?_map, List.getElem?_eq_getElem (by omega)]; rfl)
            omega
  have hlenT : (c.ctx.map (trD N) ++ c.fields.map (trT N)).length = c.ctx.length + c.fields.length := by
    simp
  refine ⟨_, .caseK he' hbc hr', .match_ te
    (.aref (Δ := fun θ'' => (c.ctx.map (trD N) ++ c.fields.map (trT N)).map (Flick.Ty.substT θ''))
      hS (trT_scrut hR hK us σ) (hbc.noIns N) hfr (fun l h => absurd h (hbc.has N _))
      (fun θ'' _ _ => ?_) (fun θ'' _ hu => ?_) (.acons .wild tr .anil))
    (.inl fun _ _ => ⟨.wild, by simp [Flick.Tm.unguarded], rfl⟩)⟩
  · have := PatsTy.vars (S := S) ((c.ctx.map (trD N) ++ c.fields.map (trT N)).map (Flick.Ty.substT θ''))
    rwa [List.length_map, hlenT] at this
  · -- `κ`: the unifier of the open problem, which the most general one is a part of
    let κ : Atom → Flick.Ty := fun a => (liftT c.k (trσ N σ) a).substT θ''
    have hκ₀ := hU.most (cunif_of hR hK hlen hu)
    have hκ : ∀ a, Flick.Ty.Equiv ((trσ N θ a).substT κ) (κ a) := by
      intro a
      cases a with
      | v n => exact hκ₀ n
      | r p => exact .refl
    have hatom : ∀ a, Flick.Ty.Equiv
        ((shiftV c.k a).substT (fun b => (trσ N θ b).substT κ))
        ((trσ N σ a).substT fun b => (shiftV c.k b).substT θ'') := by
      intro a
      have he := (shiftV c.k a).substT_equiv hκ
      have hshift : (shiftV c.k a).substT κ =
          (trσ N σ a).substT (fun b => (shiftV c.k b).substT θ'') := by
        dsimp only [κ]
        rw [← Flick.Ty.substT_comp, shift_lift, ← Flick.Ty.substT_comp, Flick.Ty.up_def]
      rw [hshift] at he
      exact he
    have hup : ∀ a, trσ N (up c.k) a = shiftV c.k a := trσ_up N c.k
    have hTy : ∀ X : Flick.Ty, Flick.Ty.Equiv
        (((X.substT (trσ N (up c.k))).substT (trσ N θ)).substT κ)
        (((X.substT (trσ N σ)).substT (shiftV c.k)).substT θ'') := by
      intro X
      simp only [Flick.Ty.substT_comp]
      apply Flick.Ty.substT_equiv
      intro a; rw [hup a]; simpa only [Flick.Ty.substT_comp] using hatom a
    have hSch : ∀ X : Flick.Scheme, Flick.Scheme.Equiv
        (((X.substT (trσ N (up c.k))).substT (trσ N θ)).substT κ)
        (((X.substT (trσ N σ)).substT (shiftV c.k)).substT θ'') := by
      intro X
      simp only [Flick.Scheme.substT_comp]
      apply Flick.Scheme.Equiv.of_subst
      intro a; rw [hup a]; simpa only [Flick.Ty.substT_comp] using hatom a
    have hts := S.ctor_closed _ _ _ _ (liftT c.k (trσ N σ)) hS fun i hi => by simp [liftT, hi]
    have hfix : ∀ f ∈ c.ctx.map (trD N) ++ c.fields.map (trT N),
        Flick.Ty.Equiv ((f.substT (trσ N θ)).substT κ) (f.substT θ'') := by
      intro f hf
      rw [Flick.Ty.substT_comp]
      have he := f.substT_equiv hκ
      have hclosed : f.substT κ = f.substT θ'' := by
        dsimp only [κ]
        rw [← Flick.Ty.substT_comp,
          (List.map_inj_left.mp (hts.trans (List.map_id _).symm)) f hf]
        rfl
      rw [hclosed] at he
      exact he
    -- the arm's context: the pattern's variables, then `Γc` past the constructor's variables
    have hΓa₁ : ∀ (j : Nat) (f : Flick.Ty), (c.ctx.map (trD N) ++ c.fields.map (trT N))[j]? = some f →
        (((c.ctx.map (trD N) ++ c.fields.map (trT N)).map (Flick.Ty.substT θ'')).map
          Flick.Scheme.mono ++ (Γc.map (Flick.Scheme.up c.k)).map (Flick.Scheme.substT θ''))[j]? =
          some (Flick.Scheme.mono (f.substT θ'')) := by
      intro j f hf
      rw [List.getElem?_append_left (by simpa using lt_of_some hf), List.getElem?_map,
        List.getElem?_map, hf]
      rfl
    have hΓa₂ : ∀ (j : Nat) (u : Flick.Scheme), Γc[j]? = some u →
        (((c.ctx.map (trD N) ++ c.fields.map (trT N)).map (Flick.Ty.substT θ'')).map
          Flick.Scheme.mono ++ (Γc.map (Flick.Scheme.up c.k)).map (Flick.Scheme.substT θ''))[j +
            (c.ctx.length + c.fields.length)]? = some ((u.up c.k).substT θ'') := by
      intro j u hu
      rw [List.getElem?_append_right (by simp), List.getElem?_map, List.getElem?_map]
      simp [hu]
    have hF : Fits (κ) bu :=
      ⟨fun p hp => (hbu.has N _ hp).elim, fun hi => (hbu.has N _ hi).elim,
        fun hi => (hbu.has N _ hi).elim, fun hi => (hbu.has N _ hi).elim⟩
    have t₁ := tbu.substT _ hF
    simp only [List.map_nil, Facts.none_substT, Flick.J.substT] at t₁
    have t₂ := (t₁.weakenStore ((Ψ.map (Flick.Ty.up c.k)).map (Flick.Ty.substT θ''))).cut
      (Θ₂ := (Θ.substT (shiftV c.k)).substT θ'') (Entails.none _)
    simp only [List.nil_append] at t₂
    have eJ₁ : Flick.Ty.Equiv
        ((trTT N (.mono ((t.subst (up c.k)).subst θ))).substT κ)
        (((trTT N (.mono (t.subst σ))).up c.k).substT θ'') := by
      simp only [trTT]; rw [trT_subst, trT_subst, trT_subst, Flick.Ty.up_def]; exact hTy (trT N t)
    have eJ₂ : (trStage N ((φ.subst (up c.k)).subst θ)).substT
        (κ) =
        ((trStage N (φ.subst σ)).substT (shiftV c.k)).substT θ'' := by
      rw [trStage_subst, trStage_subst, trStage_subst]
      simp only [Flick.Stage.substT_comp]
      apply Flick.Stage.substT_equiv
      intro a; rw [hup a]; simpa only [Flick.Ty.substT_comp] using hatom a
    have t₃ := t₂.conv eJ₁
    rw [eJ₂] at t₃
    refine t₃.subst_congruent_entries (hbu.noIns N) (fun i s hi => ?_)
    obtain ⟨s₀, hs₀, rfl⟩ := map_get hi
    have hDl : (armΔ c θ (Δl ++ Δ)).length = c.ctx.length + Δl.length + Δ.length := hD
    rcases canonC_get hs₀ with ⟨hiD, q, hq, rfl⟩ | ⟨hiD, u, hu', rfl⟩
    · unfold armΔ at hq
      rcases app_get hq with ⟨him, hq'⟩ | ⟨him, hq'⟩
      · -- a dictionary of the value
        obtain ⟨q₀, hq₀, rfl⟩ := map_get hq'
        simp only [List.length_map] at him
        left
        refine ⟨i, _, by simp [armτ, him, show i < (armΔ c θ (Δl ++ Δ)).length +
          (armT c θ Γ).length by omega],
          hΓa₁ i (trD N q₀) (by
            rw [List.getElem?_append_left (by simpa), List.getElem?_map, hq₀]; rfl), ?_⟩
        simp only [Flick.Scheme.mono_substT, trD_subst]
        exact .mono (hfix _ (List.mem_append_left _
          (List.mem_map.mpr ⟨q₀, List.mem_of_getElem? hq₀, rfl⟩)))
      · obtain ⟨q₁, hq₁, rfl⟩ := map_get hq'
        obtain ⟨q₀, hq₀, rfl⟩ := map_get hq₁
        simp only [List.length_map] at him hq₀
        rcases app_get hq₀ with ⟨hiL, hq₂⟩ | ⟨hiL, hq₂⟩
        · -- a premise of `Δl`
          left
          refine ⟨δ (i - c.ctx.length) + (c.ctx.length + c.fields.length), _, ?_,
            hΓa₂ _ _ (by rw [hc.dict, List.getElem?_map, hq₂]; rfl), ?_⟩
          · simp [armτ, show ¬ i < c.ctx.length by omega, show i < c.ctx.length + Δl.length by omega,
              show i < (armΔ c θ (Δl ++ Δ)).length + (armT c θ Γ).length by omega]
          · simpa only [Flick.Scheme.up, Flick.Scheme.mono_substT, trD_subst] using
              Flick.Scheme.Equiv.mono (hTy (trD N q₀))
        · -- a closed premise
          right
          have hτ : armτ N c.ctx.length c.fields.length Δl.length (armΔ c θ (Δl ++ Δ)).length
              (armT c θ Γ).length γ δ ρ i = trDt N (fun _ => 0) (ρ (i - c.ctx.length - Δl.length)) := by
            simp [armτ, show ¬ i < c.ctx.length by omega, show ¬ i < c.ctx.length + Δl.length by omega,
              show i < (armΔ c θ (Δl ++ Δ)).length by omega,
              show i < (armΔ c θ (Δl ++ Δ)).length + (armT c θ Γ).length by omega,
              show i - (c.ctx.length + Δl.length) = i - c.ctx.length - Δl.length by omega]
          rw [hτ]
          refine ⟨trDt_noFree (hcl _), fun a => (TrD.total (N := N) _ _).has N a, ?_, rfl, _, _, rfl⟩
          rw [Flick.Scheme.mono_substT, trD_subst, trD_subst]
          have he := hTy (trD N q₀)
          have hd := closed_dict hR (hρ _ _ hq₂) (fun a => (shiftV c.k a).substT θ'')
          have he' : Flick.Ty.Equiv
              ((trD N (q₀.subst σ)).substT (fun a => (shiftV c.k a).substT θ''))
              ((((trD N q₀).substT (trσ N (up c.k))).substT (trσ N θ)).substT κ) := by
            simpa only [trD_subst, Flick.Ty.substT_comp] using he.symm
          exact (hd _ _ _).conv he'
    · unfold armT at hu'
      rcases app_get hu' with ⟨hin, hu₁⟩ | ⟨hin, hu₁⟩
      · -- a field of the value
        obtain ⟨f₀, hf₀, rfl⟩ := map_get hu₁
        simp only [List.length_map] at hin
        have hx := lt_of_some hf₀
        left
        refine ⟨c.ctx.length + (i - (armΔ c θ (Δl ++ Δ)).length), _, ?_,
          hΓa₁ _ (trT N f₀) (by
            rw [List.getElem?_append_right (by simp), List.getElem?_map,
              show c.ctx.length + (i - (armΔ c θ (Δl ++ Δ)).length) - (c.ctx.map (trD N)).length =
                i - (armΔ c θ (Δl ++ Δ)).length by simp, hf₀]
            rfl), ?_⟩
        · simp [armτ, show ¬ i < c.ctx.length by omega, show ¬ i < c.ctx.length + Δl.length by omega,
            show ¬ i < (armΔ c θ (Δl ++ Δ)).length by omega,
            show i < (armΔ c θ (Δl ++ Δ)).length + c.fields.length by omega,
            show i < (armΔ c θ (Δl ++ Δ)).length + (armT c θ Γ).length by omega]
        · change Flick.Scheme.Equiv
            ((Flick.Scheme.mono (trT N (f₀.subst θ))).substT κ) _
          rw [Flick.Scheme.mono_substT, trT_subst]
          exact .mono (hfix _ (List.mem_append_right _
            (List.mem_map.mpr ⟨f₀, List.mem_of_getElem? hf₀, rfl⟩)))
      · -- a variable of `Γ`
        obtain ⟨u₁, hu₂, rfl⟩ := map_get hu₁
        obtain ⟨u₀, hu₀, rfl⟩ := map_get hu₂
        simp only [List.length_map] at hin hu₀
        have hy := lt_of_some hu₀
        left
        refine ⟨γ (i - (armΔ c θ (Δl ++ Δ)).length - c.fields.length) +
          (c.ctx.length + c.fields.length), _, ?_,
          hΓa₂ _ _ (by rw [hc.var, List.getElem?_map, hu₀]; rfl), ?_⟩
        · simp [armτ, show ¬ i < c.ctx.length by omega, show ¬ i < c.ctx.length + Δl.length by omega,
            show ¬ i < (armΔ c θ (Δl ++ Δ)).length by omega,
            show ¬ i < (armΔ c θ (Δl ++ Δ)).length + c.fields.length by omega,
            show i < (armΔ c θ (Δl ++ Δ)).length + (armT c θ Γ).length by omega]
        · simpa only [trS_subst, Flick.Scheme.up] using hSch (trS N u₀)

end Flick.Elab
