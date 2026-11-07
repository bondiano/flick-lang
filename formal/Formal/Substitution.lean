import Formal.SubstArm

/-!
# Substitution lemma (L2)

The substituted terms and what they keep are in `SubOk`, the arms on the
constructors of a GADT in `SubstArm`.
-/

namespace Flick

variable {S : Sig}

/-- The arms of a handler keep their operations under a substitution. -/
theorem Typ.armOf_subst {σ : Nat → Tm} {o : Name} :
    ∀ {al : Tm} {Ψ Θ Γ E t φ s}, Typ S Ψ Θ Γ al (.harms E t φ s) →
      ((al.subst σ).armOf o).isSome = (al.armOf o).isSome
  | .acons p b r, _, _, _, _, _, _, _, h => by
    have hr : ∃ Ψ Θ Γ E t φ s, Typ S Ψ Θ Γ r (.harms E t φ s) := by
      cases h <;> exact ⟨_, _, _, _, _, _, _, ‹_›⟩
    obtain ⟨_, _, _, _, _, _, _, hr⟩ := hr
    simp only [Tm.subst, Tm.armOf]
    split <;> simp [Typ.armOf_subst hr]
  | .anil, _, _, _, _, _, _, _, _ => rfl
  | .var _, _, _, _, _, _, _, _, h | .lam _, _, _, _, _, _, _, _, h | .app _ _, _, _, _, _, _, _, _, h
  | .let_ _ _, _, _, _, _, _, _, _, h | .letrec _ _, _, _, _, _, _, _, _, h
  | .fix _, _, _, _, _, _, _, _, h | .lit _, _, _, _, _, _, _, _, h | .loc _, _, _, _, _, _, _, _, h
  | .const _, _, _, _, _, _, _, _, h | .ctor _, _, _, _, _, _, _, _, h | .rnil, _, _, _, _, _, _, _, h
  | .rcons _ _ _, _, _, _, _, _, _, _, h | .proj _ _, _, _, _, _, _, _, _, h
  | .update _ _ _, _, _, _, _, _, _, _, h | .match_ _ _, _, _, _, _, _, _, _, h
  | .aguard _ _ _ _, _, _, _, _, _, _, _, h | .todo, _, _, _, _, _, _, _, h
  | .task _, _, _, _, _, _, _, _, h | .await _, _, _, _, _, _, _, _, h | .raise _, _, _, _, _, _, _, _, h
  | .try_ _ _, _, _, _, _, _, _, _, h | .handle _ _, _, _, _, _, _, _, _, h
  | .scope _, _, _, _, _, _, _, _, h | .inside _ _, _, _, _, _, _, _, _, h
  | .cap _, _, _, _, _, _, _, _, h | .own _, _, _, _, _, _, _, _, h | .drec _ _, _, _, _, _, _, _, _, h
  | .glob _, _, _, _, _, _, _, _, h | .hdl _ _ _ _, _, _, _, _, _, _, _, h
  | .hwith _ _ _ _ _ _, _, _, _, _, _, _, _, h | .hctl _ _ _ _ _ _ _ _ _, _, _, _, _, _, _, _, h
  | .hkeep _ _ _ _ _, _, _, _, _, _, _, _, h | .hkarm _ _ _ _ _ _, _, _, _, _, _, _, _, h
  | .hres _ _ _ _, _, _, _, _, _, _, _, h | .hdone _ _, _, _, _, _, _, _, _, h
  | .unw _ _ _, _, _, _, _, _, _, _, h | .kont _ _, _, _, _, _, _, _, _, h | .hole _, _, _, _, _, _, _, _, h => by
    cases h

theorem Typ.subst {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ {Γ' σ}, SubOk S Ψ Θ Γ Γ' σ → Typ S Ψ Θ Γ' (e.subst σ) J := by
  induction h with
  | var hi hts hH hT =>
    intro _ _ hσ
    obtain ⟨ht, hnI, _, hV⟩ := hσ _ _ hi
    exact (Typ.elim (hV.stands _) hnI (Typ.instantiate hts hH hnI ht) hT).sub ⟨.refl, Stage.pure_le _⟩
  | allVar hi hk hc =>
    intro _ σ hσ
    obtain ⟨ht, _, _, hV⟩ := hσ _ _ hi
    show Typ _ _ _ _ (σ _) _
    rcases hV with hv | ⟨j, hj, hs⟩ | hj | ⟨hk', -⟩
    · refine .allI hv (ht.cut fun _ _ h x hx => (h x hx).imp_right fun ⟨C', hu, h₂⟩ => ⟨C', ?_, h₂⟩)
      exact hu.elim id fun h => (hc _ _ h).elim
    · rw [hj]; exact .allVar hs hk hc
    · rw [hj]; exact .todo
    · omega
  | allI hv _ ih => intro _ _ hσ; exact .allI hv.subst (ih ((SubOk.up hσ _ _).mono Entails.union_none))
  | lam hb hnb hcb hT ih =>
    intro _ _ hσ
    have hΓ : ∀ i, (Tm.lam _).Free i → i < _ := fun i h => by
      have := hb.frees _ h; simp at this; omega
    exact .lam (ih (SubOk.lift hσ [_] rfl)) (SubOk.noIns (e := .lam _) hσ hΓ hnb) (SubOk.held hσ hΓ hcb) hT
  | app _ _ hψ hT ih₁ ih₂ => intro _ _ hσ; exact .app (ih₁ hσ) (ih₂ hσ) hψ hT
  | let_ _ _ ih₁ ih₂ => intro _ _ hσ; exact .let_ (ih₁ hσ) (ih₂ (SubOk.lift hσ [_] rfl))
  | letGen _ _ ih₁ ih₂ =>
    intro _ _ hσ; exact .letGen (ih₁ (SubOk.up hσ _ _)) (ih₂ (SubOk.lift hσ [_] rfl))
  | letrec hf hnf hcf _ ih₁ ih₂ =>
    intro _ _ hσ
    have hΓ : ∀ i, (Tm.fix _).Free i → i < _ := fun i h => by
      have := hf.frees _ h; simp at this; omega
    exact .letrec (ih₁ (SubOk.lift hσ [_, _] rfl)) (SubOk.noIns (e := .fix _) hσ hΓ hnf)
      (SubOk.held hσ hΓ hcf) (ih₂ (SubOk.lift hσ [_] rfl))
  | fix hf hnf hcf hT ih =>
    intro _ _ hσ
    have hΓ : ∀ i, (Tm.fix _).Free i → i < _ := fun i h => by
      have := hf.frees _ h; simp at this; omega
    exact .fix (ih (SubOk.lift hσ [_, _] rfl)) (SubOk.noIns (e := .fix _) hσ hΓ hnf) (SubOk.held hσ hΓ hcf) hT
  | lit => intro _ _ _; exact .lit
  | loc hl => intro _ _ _; exact .loc hl
  | const hc hT => intro _ _ _; exact .const hc hT
  | ctor hK hk hH hT => intro _ _ _; exact .ctor hK hk hH hT
  | rnil => intro _ _ _; exact .rnil
  | rcons hR hl _ _ ih₁ ih₂ => intro _ _ hσ; exact .rcons hR hl (ih₁ hσ) (ih₂ hσ)
  | proj _ hf hT ih => intro _ _ hσ; exact .proj (ih hσ) hf hT
  | update _ hf _ ih₁ ih₂ => intro _ _ hσ; exact .update (ih₁ hσ) hf (ih₂ hσ)
  | match_ _ h₂ hc ih₁ ih₂ =>
    intro _ σ hσ
    exact .match_ (ih₁ hσ) (ih₂ hσ)
      (by rw [Typ.unguarded_subst h₂ σ rfl]; exact hc.map (Homo.subst σ))
  | anil => intro _ _ _; exact .anil
  | acons hp _ _ ih₁ ih₂ =>
    intro _ _ hσ; exact .acons hp (ih₁ (SubOk.lift hσ _ (by simpa using hp.binds))) (ih₂ hσ)
  | aguard hp _ _ _ ih₁ ih₂ ih₃ =>
    intro _ _ hσ
    exact .aguard hp (ih₁ (SubOk.lift hσ _ (by simpa using hp.binds)))
      (ih₂ (SubOk.lift hσ _ (by simpa using hp.binds))) (ih₃ hσ)
  | aref hK hU hn hfr hlc hpA _ _ ihA ihr =>
    intro _ _ hσ; exact Typ.aref_subst hK hU hσ hn hfr hlc hpA ihA (ihr hσ)
  | todo => intro _ _ _; exact .todo
  | task he hψ hne hce hT ih =>
    intro _ _ hσ
    exact .task (ih hσ) hψ (SubOk.noIns (e := .task _) hσ he.frees hne) (SubOk.held hσ he.frees hce) hT
  | await _ hφ hT ih => intro _ _ hσ; exact .await (ih hσ) hφ hT
  | raise _ hφ ih => intro _ _ hσ; exact .raise (ih hσ) hφ
  | raiseAdt _ hU hφ ih => intro _ _ hσ; exact .raiseAdt (ih hσ) hU hφ
  | errVal hv _ hU hE ih => intro _ _ hσ; exact .errVal hv.subst (ih hσ) hU hE
  | tryR _ _ hφ hs ih₁ ih₂ => intro _ _ hσ; exact .tryR (ih₁ hσ) (ih₂ hσ) hφ hs
  | tryP _ _ ih₁ ih₂ => intro _ _ hσ; exact .tryP (ih₁ hσ) (ih₂ hσ)
  | handle hv _ _ hc ha hφ ih₁ ih₂ =>
    intro _ σ hσ
    exact .handle hv.subst (ih₁ hσ) (ih₂ hσ) (hc.map (Homo.subst σ) hv) ha hφ
  | hnil hH => intro _ _ _; exact .hnil hH
  | hcons hp _ _ hH ih₁ ih₂ =>
    intro _ _ hσ; exact .hcons hp (ih₁ (SubOk.lift hσ _ (by simpa using hp.binds))) (ih₂ hσ) hH
  | hguard hp _ _ _ hH ih₁ ih₂ ih₃ =>
    intro _ _ hσ
    exact .hguard hp (ih₁ (SubOk.lift hσ _ (by simpa using hp.binds)))
      (ih₂ (SubOk.lift hσ _ (by simpa using hp.binds))) (ih₃ hσ) hH
  | scope hne hF he hφ' hφ hT ih =>
    intro _ _ hσ
    have hΓ : ∀ i, (Tm.scope _).Free i → i < _ := fun i h => by
      have := he.frees _ h; simp at this; omega
    exact .scope (SubOk.noIns (e := .scope _) hσ hΓ hne) hF (ih (SubOk.lift (SubOk.up hσ 1 _) [_] rfl)) hφ' hφ hT
  | inside hF _ hφ' ht hε hφ hT ih =>
    intro _ _ hσ; exact .inside hF (ih (hσ.mono (Entails.union_left _ _))) hφ' ht hε hφ hT
  | cap => intro _ _ _; exact .cap
  | own => intro _ _ _; exact .own
  | drec hc hk hr hn hh ih =>
    intro _ _ hσ
    exact .drec hc hk (ih hσ) (SubOk.noIns hσ hr.frees hn) (SubOk.held (e := .drec _ _) hσ hr.frees hh)
  | dnil => intro _ _ _; exact .dnil
  | dcons _ _ ih₁ ih₂ => intro _ _ hσ; exact .dcons (ih₁ (SubOk.up hσ _ _)) (ih₂ hσ)
  | dproj _ hc hk hl hts hH hT ih => intro _ _ hσ; exact .dproj (ih hσ) hc hk hl hts hH hT
  | glob hg hts hH hT => intro _ _ _; exact .glob hg hts hH hT
  | perform hop hT => intro _ _ _; exact .perform hop hT
  | hanil => intro _ _ _; exact .hanil
  | hatail hop _ _ ih₁ ih₂ => intro _ _ hσ; exact .hatail hop (ih₁ (SubOk.lift hσ [_] rfl)) (ih₂ hσ)
  | haabort hop _ _ ih₁ ih₂ => intro _ _ hσ; exact .haabort hop (ih₁ (SubOk.lift hσ [_] rfl)) (ih₂ hσ)
  | hactl hop _ _ ih₁ ih₂ => intro _ _ hσ; exact .hactl hop (ih₁ (SubOk.lift hσ [_, _] rfl)) (ih₂ hσ)
  | hdl hal hn _ hψ _ hφ₀ hOb hc hφ hT ih₁ ih₂ ih₃ =>
    intro _ σ hσ
    have h₁ := ih₁ ((SubOk.up hσ 1 Facts.none).mono fun C s h x hx =>
      (h x hx).imp id fun ⟨C', h₁, h₂⟩ => ⟨C', h₁.elim id False.elim, h₂⟩)
    exact .hdl h₁ (SubOk.noIns hσ (by simpa using hal.frees) hn) (ih₂ hσ) hψ (ih₃ hσ) hφ₀ hOb ⟨hc.1, fun h => hc.2 fun hc' => h fun o c a b ho => by
      rw [hal.armOf_subst]; exact hc' o c a b ho⟩ hφ hT
  | hwith hℓ hal _ hψ _ hφ₀ hc hφ hT ih₁ ih₂ ih₃ =>
    intro _ σ hσ
    exact .hwith hℓ (ih₁ hσ) (ih₂ hσ) hψ (ih₃ hσ) hφ₀ ⟨hc.1, fun h => hc.2 fun hc' => h fun o c a b ho => by
      rw [hal.armOf_subst]; exact hc' o c a b ho⟩ hφ hT
  | hctl hℓ hal _ hψ hop hc htl hE hk hp hφ₀ _ hl hcm hφ hT ih₁ ih₂ _ ih₄ =>
    intro _ σ hσ
    subst hk
    rw [Tm.subst, (plug_hole_closed hp).subst]
    exact .hctl hℓ (ih₁ hσ) (ih₂ hσ) hψ hop hc htl hE rfl hp hφ₀ (ih₄ hσ) hl ⟨hcm.1, fun h => hcm.2 fun hc' => h fun o c a b ho => by
      rw [hal.armOf_subst]; exact hc' o c a b ho⟩ hφ hT
  | hkeep hℓ hal _ hφ₀ hc hnt hle hφ hT ih₁ ih₂ =>
    intro _ σ hσ
    exact .hkeep hℓ (ih₁ hσ) (ih₂ hσ) hφ₀ ⟨hc.1, fun h => hc.2 fun hc' => h fun o c a b ho => by
      rw [hal.armOf_subst]; exact hc' o c a b ho⟩ hnt hle hφ hT
  | hkarm hℓ hal hop hE hk hp hφ₀ hc hnt _ hle hφ hT ih₁ _ ih₃ =>
    intro _ σ hσ
    subst hk
    rw [Tm.subst, (plug_hole_closed hp).subst]
    exact .hkarm hℓ (ih₁ hσ) hop hE rfl hp hφ₀ ⟨hc.1, fun h => hc.2 fun hc' => h fun o c a b ho => by
      rw [hal.armOf_subst]; exact hc' o c a b ho⟩ hnt (ih₃ hσ) hle hφ hT
  | hres hℓ _ hA ha hp hl hφ hT ih₁ _ =>
    intro _ σ hσ
    subst ha
    rw [Tm.subst, (plug_hole_closed hp).subst]
    exact .hres hℓ (ih₁ hσ) hA rfl hp hl hφ hT
  | hdone _ ih => intro _ _ hσ; exact .hdone (ih hσ)
  | unw hℓ _ hφ ih => intro _ _ hσ; exact .unw hℓ (ih hσ) hφ
  | kont hl hC hT => intro _ _ _; exact .kont hl hC hT

/-- `e[x := v]` for `x` with a scheme: `v` is a value typed with the scheme's
variables fresh and its facts assumed. -/
theorem SubOk.one {Ψ Θ v} {s : Scheme} (hv : Value v)
    (h : Typ S (Ψ.map (Ty.up s.k)) (Θ.at s) [] v (.ex s.t .pure)) :
    SubOk S Ψ Θ [s] [] (inst [v]) := by
  intro i s' hi
  cases i with
  | zero => simp at hi; subst hi; exact ⟨h, SubOk.value hv h⟩
  | succ i => simp at hi

/-- `δ` gives the variables `Δ` their types in `Γ`, with `P`: values. -/
def Inst (S : Sig) (Ψ : List Ty) (Θ : Facts) (Γ : List Scheme) (δ : List Tm) (Δ : List Ty) : Prop :=
  δ.length = Δ.length ∧
    ∀ (i : Nat) v t, δ[i]? = some v → Δ[i]? = some t → Value v ∧ Typ S Ψ Θ Γ v (.ex t .pure)

theorem Inst.nil {Ψ Θ Γ} : Inst S Ψ Θ Γ [] [] := ⟨rfl, by simp⟩

theorem Inst.cons {Ψ Θ Γ v a δ Δ} (hvv : Value v) (hv : Typ S Ψ Θ Γ v (.ex a .pure))
    (h : Inst S Ψ Θ Γ δ Δ) : Inst S Ψ Θ Γ (v :: δ) (a :: Δ) := by
  refine ⟨by simp [h.1], fun i w t hw ht => ?_⟩
  cases i with
  | zero => simp at hw ht; subst hw ht; exact ⟨hvv, hv⟩
  | succ i => exact h.2 i w t (by simpa using hw) (by simpa using ht)

theorem Inst.append {Ψ Θ Γ δ₁ δ₂ Δ₁ Δ₂} (h₁ : Inst S Ψ Θ Γ δ₁ Δ₁) (h₂ : Inst S Ψ Θ Γ δ₂ Δ₂) :
    Inst S Ψ Θ Γ (δ₁ ++ δ₂) (Δ₁ ++ Δ₂) := by
  refine ⟨by simp [h₁.1, h₂.1], fun i w t hw ht => ?_⟩
  have hl := h₁.1
  by_cases hi : i < δ₁.length
  · rw [List.getElem?_append_left hi] at hw
    rw [List.getElem?_append_left (by omega)] at ht
    exact h₁.2 i w t hw ht
  · rw [List.getElem?_append_right (by omega)] at hw
    rw [List.getElem?_append_right (by omega)] at ht
    rw [h₁.1] at hw
    exact h₂.2 _ w t hw ht

theorem Inst.ok {Ψ Θ Γ δ Δ} (h : Inst S Ψ Θ Γ δ Δ) :
    SubOk S Ψ Θ (Δ.map .mono ++ Γ) Γ (inst δ) := by
  intro i s hi
  have hl := h.1
  unfold inst
  split
  · next hlt =>
    rw [List.getElem?_append_left (by simp; omega)] at hi
    simp only [List.getElem?_map, Option.map_eq_some_iff] at hi
    obtain ⟨t, ht, rfl⟩ := hi
    obtain ⟨hvv, hv⟩ := h.2 i _ t (List.getElem?_eq_getElem hlt) ht
    have hv' : Typ S (Ψ.map (Ty.up (Scheme.mono t).k)) (Θ.at (.mono t))
        (Γ.map (Scheme.up (Scheme.mono t).k)) δ[i] (.ex (Scheme.mono t).t .pure) := by
      simp only [Scheme.mono_k, List.map_up_zero, List.map_sup_zero, Facts.at, Scheme.mono_t,
        Scheme.mono_cs]
      refine hv.cut fun C r hC => hC.mono fun C r h => .inl ?_
      have : shiftV 0 = Ty.ident := by funext a; cases a <;> rfl
      rw [this, Facts.substT_ident]; exact h
    exact ⟨hv', SubOk.value hvv hv'⟩
  · rw [List.getElem?_append_right (by simp; omega)] at hi
    have hi' : Γ[i - δ.length]? = some s := by simpa [h.1] using hi
    refine ⟨Typ.var_self hi', ?_, ?_, .inr (.inl ⟨_, rfl, hi'⟩)⟩
    · intro h; cases h
    rintro a (⟨j, s', hj, hs, ha⟩ | ⟨p, hp, _⟩ | ⟨l, _, hl, _⟩)
    · cases hj; rw [hi'] at hs; cases hs; exact ha
    · cases hp
    · cases hl

/-- `e[δ]` for a body typed under `Δ`. -/
theorem Typ.inst {Ψ Θ Γ e J δ Δ} (h : Typ S Ψ Θ (Δ.map .mono ++ Γ) e J) (hδ : Inst S Ψ Θ Γ δ Δ) :
    Typ S Ψ Θ Γ (e.subst (inst δ)) J :=
  h.subst hδ.ok

end Flick
