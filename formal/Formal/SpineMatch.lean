import Formal.Patterns

/-!
# Matching a constructor spine at the instance of its value

An arm on a constructor of a GADT types the patterns of the fields at an instance of
the constructor's variables. A value built by the constructor carries its own
instance, from its (Ctor) premise: that instance unifies the arm's problem, and the
patterns of the fields at it give the bindings of the match.
-/

namespace Flick

variable {S : Sig}

/-- A constructor spine at the instance `us` of the variables of its constructor `K`. -/
inductive SpTy (S : Sig) (K : Name) (us : List Ty) : Pat → Ty → List Ty → Prop
  | head {k : Nat} {ts : List Ty} {T : Name} : S.ctor K = some (k, ts, T) →
      SpTy S K us (.ctor K) (Sig.ctorTyR ts T (S.res K) us) []
  | app {p q : Pat} {a t : Ty} {Δ₁ Δ₂ : List Ty} : SpTy S K us p (.arrow a .pure t Caps.none) Δ₁ →
      PatTy S true q a Δ₂ → SpTy S K us (.app p q) t (Δ₁ ++ Δ₂)

theorem PatsTy.spTy {K : Name} {us qs xs Δ} (h : PatsTy S qs xs Δ) :
    ∀ {p r Δ₀}, SpTy S K us p (Ty.arrows xs r) Δ₀ → SpTy S K us (qs.foldl .app p) r (Δ₀ ++ Δ) := by
  induction h with
  | nil => intro p r Δ₀ h; simpa using h
  | cons hq _ ih => intro p r Δ₀ h; simpa [List.foldl, List.append_assoc] using ih (.app h hq)

theorem matchP_foldl_head {h : Tm} : ∀ (qs : List Pat) (p : Pat) {v δ},
    (∀ v δ, matchP p v = some δ → v.head = h) → matchP (qs.foldl .app p) v = some δ → v.head = h
  | [], _, _, _, hp, hm => hp _ _ hm
  | q :: qs, p, _, _, hp, hm => matchP_foldl_head qs (.app p q) (fun v δ hm' => by
      cases v with
      | app f e =>
        obtain ⟨δ₁, _, h₁, -⟩ := matchP_app.mp hm'
        simp only [Tm.head]; exact hp f δ₁ h₁
      | _ => simp [matchP] at hm') hm

theorem matchP_spine_head {K qs v δ} (hm : matchP (Pat.spine K qs) v = some δ) :
    v.head = .ctor K :=
  matchP_foldl_head qs (.ctor K) (fun v _ h => by
    cases v <;> simp [matchP] at h; simp [Tm.head, h.1]) hm

/-- A value built by a constructor `K` at an instance `us` of its variables: the final
type of the value is `T rs` at `us`, its hidden row perhaps widened, and patterns of the
fields at `us` or at a widening of it match. -/
theorem Typ.spine_match {Ψ Θ Γ v J} (h : Typ S Ψ Θ Γ v J) :
    Value v → ∀ {K k ts T X}, v.head = .ctor K → S.ctor K = some (k, ts, T) →
      J = .ex X .pure → X.kind ≠ 7 → ¬ X.isAll →
      ∃ us, us.length = k ∧
        (∃ ws, X.res = Ty.tcon T ws ∧ S.Vary T ((S.res K).map (Ty.substT (instT us))) ws) ∧
        ∀ {us' p Z Δ δ}, S.Vary T us us' → SpTy S K us' p Z Δ → matchP p v = some δ →
          (∃ Z', LeLive S Z' X ∧ SpW S Z' Z) ∧ Inst S Ψ Θ Γ δ Δ := by
  induction h with
  | conv _ he ih =>
    intro hv K k ts T X hK hS hJ hk hA; cases hJ
    obtain ⟨us, hl, ⟨ws, hr, hw⟩, hus⟩ := ih hv hK hS rfl
      (by rw [he.kind]; exact hk) (by rw [he.isAll]; exact hA)
    obtain ⟨vs, hv, hev⟩ := he.res_tcon hr
    refine ⟨us, hl, ⟨vs, hv, hw.trans (.of_equiv hev)⟩, fun hwu hp hm => ?_⟩
    obtain ⟨⟨Z, hZ, hs⟩, hδ⟩ := hus hwu hp hm
    exact ⟨⟨Z, hZ.trans (.equiv he), hs⟩, hδ⟩
  | ctor hK hk _ hT =>
    intro _ _ _ _ _ _ hK' hS hJ _ _
    cases hJ; simp only [Tm.head] at hK'; cases hK'
    rw [hK] at hS; cases hS
    refine ⟨_, hk, LeLive.res_tcon hT (Sig.ctorTyR_res _ _ _ _), fun hw hp hm => ?_⟩
    cases hp with
    | head hK'' =>
      rw [hK] at hK''; cases hK''
      simp [matchP] at hm; subst hm
      exact ⟨⟨_, hT, SpW.ctor hK hk hw⟩, .nil⟩
    | app => simp [matchP] at hm
  | app _ he _ hT ih₁ _ =>
    intro hv _ _ _ _ _ hK hS hJ _ _
    cases hJ
    cases hv with
    | ctorApp hvf _ hve =>
      simp only [Tm.head] at hK
      obtain ⟨us, hl, ⟨ws, hr, hw⟩, hus⟩ :=
        ih₁ hvf hK hS rfl (by simp [Ty.kind]) (by simp [Ty.isAll])
      simp only [Ty.res] at hr
      obtain ⟨ws', hr', hw'⟩ := hT.res_tcon hr
      refine ⟨us, hl, ⟨ws', hr', hw.trans hw'⟩, fun hwu hp hm => ?_⟩
      cases hp with
      | head => simp [matchP] at hm
      | app hp' hq =>
        obtain ⟨δ₁, δ₂, h1, h2, rfl⟩ := matchP_app.mp hm
        obtain ⟨⟨Z, hZ, hs⟩, i₁⟩ := hus hwu hp' h1
        obtain ⟨_, _, _, _, h, la, Z', lt, hs'⟩ := hs.arrow_step hZ
        cases h
        exact ⟨⟨Z', lt.trans hT, hs'⟩, i₁.append (hq.inst hve
          (he.sub (J' := .ex _ .pure) ⟨la, Stage.le_refl _⟩) h2)⟩
    | constApp => simp [Tm.head] at hK
  | errVal => intro _ _ _ _ _ _ _ _ hJ hk; cases hJ; simp [Ty.kind] at hk
  | allI => intro _ _ _ _ _ _ _ _ hJ _ hA; cases hJ; exact (hA trivial).elim
  | _ => intro hv _ _ _ _ _ hK; cases hv <;> simp [Tm.head] at hK

theorem Stage.up_substT (φ : Stage) (k : Nat) (us : List Ty) (h : us.length = k) :
    (φ.substT (shiftV k)).substT (instT us) = φ := by
  rw [Stage.substT_comp]; conv => rhs; rw [← φ.substT_ident]
  congr 1; funext a
  cases a with
  | v i =>
    have : ¬ i + k < k := by omega
    simp [shiftV, Ty.substT, instT, this, h, Ty.ident]
  | r p => simp [shiftV, Ty.substT, instT, Ty.ident]

theorem Ty.res_of_hd {t : Ty} {T : Name} (h : t.hd = .adt T) : t.res = t := by
  cases t <;> simp [Ty.hd] at h <;> rfl

/-- The step into an arm on a constructor of a GADT: the instance of the value's own
constructor unifies the arm's problem and types its patterns and its body. -/
theorem aref_match {Ψ Θ v t t' φ K k ts T₀ qs b δ} {Δ : (Atom → Ty) → List Ty}
    (hv : Value v) (h₁ : Typ S Ψ Θ [] v (.ex t φ)) (hK : S.ctor K = some (k, ts, T₀))
    (hU : t.hd = .adt T₀) (hn : b.NoIns)
    (hpA : ∀ θ, Fits θ b → Ty.Equiv ((Ty.tcon T₀ (S.res K)).substT θ) ((t.up k).substT θ) →
      PatsTy S qs (ts.map (Ty.substT θ)) (Δ θ))
    (hbA : ∀ θ, Fits θ b → Ty.Equiv ((Ty.tcon T₀ (S.res K)).substT θ) ((t.up k).substT θ) →
      Typ S ((Ψ.map (Ty.up k)).map (Ty.substT θ)) ((Θ.substT (shiftV k)).substT θ)
        ((Δ θ).map .mono ++ (([] : List Scheme).map (Scheme.up k)).map (Scheme.substT θ)) b
        (.ex ((t'.up k).substT θ) ((φ.substT (shiftV k)).substT θ)))
    (hm : matchP (Pat.spine K qs) v = some δ) :
    Typ S Ψ Θ [] (b.subst (inst δ)) (.ex t' φ) := by
  have hU7 : t.kind ≠ 7 := by rw [Ty.kind_of_hd hU]; decide
  obtain ⟨us₀, hk₀, ⟨ws, hr, hw⟩, hus⟩ :=
    (h₁.valueAny hv .pure).spine_match hv (matchP_spine_head hm) hK rfl hU7 (Ty.not_all_of_hd hU)
  rw [Ty.res_of_hd hU] at hr
  -- the arm's problem is solved at the value's instance with its hidden row widened to `t`
  obtain ⟨us, hk, hmap, hwu⟩ := Sig.Vary.lift hK hk₀ hw
  have hF : Fits (instT us) b := ⟨fun _ _ => rfl, fun h => (hn h).elim, fun h => (hn h).elim,
    fun h => (hn h).elim⟩
  have hu : Ty.Equiv ((Ty.tcon T₀ (S.res K)).substT (instT us))
      ((t.up k).substT (instT us)) := by
    rw [Ty.up_substT t k us hk, Ty.tcon_substT, hr]
    exact hmap.tcon T₀
  have hsp := (hpA _ hF hu).spTy (K := K) (us := us) (.head hK)
  obtain ⟨-, hδ⟩ := hus hwu hsp hm
  have hb := hbA _ hF hu
  simp only [List.map_nil, List.map_map, Function.comp_def, Ty.up_substT _ _ _ hk, List.map_id',
    Facts.up_substT _ _ _ hk, Stage.up_substT _ _ _ hk] at hb
  exact hb.inst (by simpa using hδ)

end Flick
