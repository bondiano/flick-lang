import Formal.TySubstBase

/-!
# Type substitution preserves typing: the lemma

The fitting substitutions and the cases with binders are in `TySubstBase`.
-/

namespace Flick

variable {S : Sig}

theorem Typ.substT {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ σ, Fits σ e →
      Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Γ.map (Scheme.substT σ)) e (J.substT σ) := by
  induction h with
  | var hi hts hH hT => intro σ _; exact Typ.var_substT hi hts hH hT σ
  | lam _ hn hc hT ih =>
    intro σ hσ; have := ih σ hσ
    simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at this
    exact .lam this hn (held_substT (e := .lam _) hσ hc) (hT.substT σ)
  | app _ _ hψ hT ih₁ ih₂ =>
    intro σ hσ
    exact .app (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
      (Stage.le_substT σ hψ) (hT.substT σ)
  | let_ _ _ ih₁ ih₂ =>
    intro σ hσ; have := ih₂ σ (hσ.sub fun _ h => .inr h)
    simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at this
    exact .let_ (ih₁ σ (hσ.sub fun _ h => .inl h)) this
  | letGen _ _ ih₁ ih₂ => exact Typ.letGen_substT ih₁ ih₂
  | letrec _ hn hc _ ih₁ ih₂ =>
    intro σ hσ
    have h₁ := ih₁ σ (hσ.sub fun _ h => .inl h)
    simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at h₁
    have h₂ := ih₂ σ (hσ.sub fun _ h => .inr h)
    simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at h₂
    exact .letrec h₁ hn (held_substT (e := .fix _) (hσ.sub fun _ h => .inl h) hc) h₂
  | fix _ hn hc hT ih =>
    intro σ hσ; have := ih σ hσ
    simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at this
    exact .fix this hn (held_substT (e := .fix _) hσ hc) (hT.substT σ)
  | lit => intro _ _; exact .lit
  | loc hl => intro σ _; exact .loc (by simp [hl])
  | const hc hT =>
    intro σ _; exact .const (Const.ok_substT σ hc) (by have := hT.substT σ; rwa [Const.ty_substT _ _ _ _ hc] at this)
  | ctor hK hk hT => intro σ _; exact Typ.ctor_substT hK hk hT σ
  | rnil => intro _ _; exact .rnil
  | rcons hR hl _ _ ih₁ ih₂ =>
    intro σ hσ
    exact .rcons (hR.substT σ) (by rw [Ty.labels_substT σ hR]; exact hl)
      (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
  | proj _ hf hT ih => intro σ hσ; exact .proj (ih σ hσ) (Ty.field_substT σ hf) (hT.substT σ)
  | update _ hf _ ih₁ ih₂ =>
    intro σ hσ
    exact .update (ih₁ σ (hσ.sub fun _ h => .inl h)) (Ty.field_substT σ hf)
      (ih₂ σ (hσ.sub fun _ h => .inr h))
  | match_ _ _ hc ih₁ ih₂ =>
    intro σ hσ
    exact .match_ (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
      (hc.substT σ)
  | anil => intro _ _; exact .anil
  | acons hp _ _ ih₁ ih₂ =>
    intro σ hσ
    have := ih₁ σ (hσ.sub fun _ h => .inl h); rw [ctx_substT] at this
    exact .acons (hp.substT σ) this (ih₂ σ (hσ.sub fun _ h => .inr h))
  | aguard hp _ _ _ ih₁ ih₂ ih₃ =>
    intro σ hσ
    have g := ih₁ σ (hσ.sub fun _ h => .inl h); rw [ctx_substT] at g
    have b := ih₂ σ (hσ.sub fun _ h => .inr (.inl h)); rw [ctx_substT] at b
    exact .aguard (hp.substT σ) g b (ih₃ σ (hσ.sub fun _ h => .inr (.inr h)))
  | todo => intro _ _; exact .todo
  | task _ hψ hn hc hT ih =>
    intro σ hσ
    exact .task (ih σ hσ) (Stage.le_substT σ hψ) hn (held_substT (e := .task _) hσ hc)
      (hT.substT σ)
  | await _ hφ hT ih =>
    intro σ hσ; exact .await (ih σ hσ) (Stage.le_substT σ hφ) (hT.substT σ)
  | raise _ hφ ih =>
    intro σ hσ; exact .raise (ih σ hσ) (by simpa [Stage.substT] using Stage.le_substT σ hφ)
  | raiseAdt _ hU hφ ih =>
    intro σ hσ
    refine .raiseAdt (ih σ hσ) (Ty.hd_substT σ hU) ?_
    simpa [Stage.substT, Row.err_substT] using Stage.le_substT σ hφ
  | errVal hv _ hU hE ih =>
    intro σ hσ
    refine .errVal hv (ih σ hσ) (Ty.hd_substT σ hU) ?_
    rw [← Row.err_substT]; exact hE.substT σ
  | tryR _ _ hφ hs ih₁ ih₂ =>
    intro σ hσ
    exact .tryR (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
      (Stage.le_substT σ hφ) (by simpa using hs.substT σ)
  | tryP _ _ ih₁ ih₂ =>
    intro σ hσ
    have := ih₂ σ (hσ.sub fun _ h => .inr h)
    simp only [Flick.J.substT, Row.none_substT] at this
    exact .tryP (ih₁ σ (hσ.sub fun _ h => .inl h)) this
  | handle hv _ _ hc ha hφ ih₁ ih₂ =>
    intro σ hσ
    refine .handle hv (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
      ?_ ?_ (by simpa [Stage.substT] using Stage.le_substT σ hφ)
    · exact fun K k ts T h₁ h₂ => (hc K k ts T h₁ h₂).imp .inl .inl
    · simpa using Row.passes_substT σ ha
  | hnil hH =>
    intro σ _
    obtain rfl : _ = Row.none := funext fun n => propext ⟨fun h => hH n h, False.elim⟩
    rw [Flick.J.substT, Row.none_substT]; exact .hnil fun _ h => h
  | hcons hp _ _ hH ih₁ ih₂ =>
    intro σ hσ
    have := ih₁ σ (hσ.sub fun _ h => .inl h); rw [ctx_substT] at this
    exact .hcons (hp.substT σ) this (ih₂ σ (hσ.sub fun _ h => .inr h)) (Catches.substT σ hH)
  | hguard hp _ _ _ hH ih₁ ih₂ ih₃ =>
    intro σ hσ
    have g := ih₁ σ (hσ.sub fun _ h => .inl h); rw [ctx_substT] at g
    have b := ih₂ σ (hσ.sub fun _ h => .inr (.inl h)); rw [ctx_substT] at b
    exact .hguard (hp.substT σ) g b (ih₃ σ (hσ.sub fun _ h => .inr (.inr h))) (hH.substT σ)
  | provideL _ _ h₁ h₀ h₂ ih₁ ih₂ =>
    intro σ hσ
    exact .provideL (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
      (by simpa [Stage.substT] using Stage.le_substT σ h₁) (h₀.substT σ)
      (by simpa [Stage.substT] using Stage.le_substT σ h₂)
  | provideE _ _ h₁ h₀ h₂ ih₁ ih₂ =>
    intro σ hσ
    exact .provideE (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
      (by simpa [Stage.substT] using Stage.le_substT σ h₁) (h₀.substT σ)
      (by simpa [Stage.substT] using Stage.le_substT σ h₂)
  | with_ hv _ _ h₁ h₂ ih₁ ih₂ =>
    intro σ hσ
    have hq := hσ.1 _ (.inr (.inr (.inl rfl)))
    have hat := hσ.2.2.1 (.inl rfl)
    exact .with_ hv (ih₁ σ (hσ.sub fun _ h => .inr (.inr (.inl h))))
      (ih₂ σ (hσ.sub fun _ h => .inr (.inr (.inr h))))
      (Stage.le_rt_sub (by simpa [Stage.substT] using Stage.le_substT σ h₁)
        (Row.withP_substT hq hat (hσ.2.2.2 (.inl rfl))))
      (by simpa [Stage.substT] using Stage.le_substT σ h₂)
  | enil hρ =>
    intro σ _
    obtain rfl : _ = Row.none := funext fun n => propext ⟨fun h => hρ n h, False.elim⟩
    simp only [Flick.J.substT, Ty.substT, Row.none_substT]; exact .enil fun _ h => h
  | econs hv hθ _ _ hρ ih₁ ih₂ =>
    intro σ hσ
    have := ih₁ σ (hσ.sub fun _ h => .inl h); simp only [Flick.J.substT, S.impl_closed] at this
    obtain rfl : _ = Row.union (Row.single _) _ := funext fun n => propext (hρ n)
    simp only [Flick.J.substT, Ty.substT, Row.union_substT, Row.single_substT] at ih₂ ⊢
    exact .econs hv hθ this (ih₂ σ (hσ.sub fun _ h => .inr h)) fun _ => Iff.rfl
  | bind _ hρ ih =>
    intro σ hσ
    have := ih σ hσ; simp only [Flick.J.substT, S.impl_closed] at this
    obtain rfl : _ = Row.single _ := funext fun n => propext (hρ n)
    simp only [Flick.J.substT, Ty.substT, Row.single_substT]
    exact .bind this fun _ => Iff.rfl
  | join _ _ hρ ih₁ ih₂ =>
    intro σ hσ
    obtain rfl : _ = Row.union _ _ := funext fun n => propext (hρ n)
    simp only [Flick.J.substT, Ty.substT, Row.union_substT] at ih₁ ih₂ ⊢
    exact .join (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h)) fun _ => Iff.rfl
  | @scope F _ _ r ε _ _ _ _ _ _ hn hF _ hφ' hφ hT ih =>
    intro σ hσ
    have h₁ := ih (liftT 1 σ) (hσ.lift 1)
    simp only [Flick.J.substT, List.map_cons, Scheme.mono_substT, Ty.substT, liftT] at h₁
    simp only [if_true, Nat.zero_lt_one] at h₁
    rw [shift_comm, shift_comm', Facts.union_substT, Facts.up_comm, Ty.up_comm] at h₁
    refine .scope hn ?_ h₁ ?_ (Stage.le_substT σ hφ) (hT.substT σ)
    · rintro C s ⟨C₀, s₀, h₀, rfl, rfl⟩
      obtain ⟨rfl, hC⟩ := hF C₀ s₀ h₀
      refine ⟨by simp [Ty.substT, liftT], fun ⟨b, hb, hv⟩ => ?_⟩
      cases b with
      | v i =>
        cases i with
        | zero => exact hC hb
        | succ i =>
          simp only [liftT, show ¬ i + 1 < 1 by omega, if_false] at hv
          exact Ty.up_avoid_v0 _ hv
      | r q => simp only [liftT] at hv; exact Ty.up_avoid_v0 _ hv
    · have := Stage.le_substT (liftT 1 σ) hφ'
      have hd := Row.disch_substT (a := .v 0) (a' := .v 0) (r := r.substT (shiftV 1))
        (σ := liftT 1 σ) (fun x hx => by simpa [liftT, Ty.reach] using hx)
      rw [Row.up_comm] at hd
      simp only [Stage.substT, Row.union_substT, Row.scope_substT, Row.up_comm] at this
      refine Stage.le_rt_sub (r := Row.union (Row.scope (.tvar 0)) (Row.union
        ((r.substT σ).substT (shiftV 1)) ((Row.disch (.v 0) (r.substT (shiftV 1))).substT (liftT 1 σ))))
        (by simpa [Ty.substT, liftT] using this) ?_
      rintro x (hx | hx | hx)
      · exact .inl hx
      · exact .inr (.inl hx)
      · exact .inr (.inr (hd x hx))
  | inside hF _ hφ' ht hε hφ hT ih =>
    intro σ hσ
    have hp : σ (.r _) = .rg _ := hσ.1 _ (.inl (.inl rfl))
    have hav := hσ.2.1 (.inr (.inl rfl))
    have h₁ := ih σ (hσ.sub fun _ h => .inr (.inr (.inr h)))
    rw [Facts.union_substT] at h₁
    refine .inside ?_ h₁ ?_ (fun h => ht ?_) ?_ (Stage.le_substT σ hφ) (hT.substT σ)
    · rintro C s ⟨C₀, s₀, h₀, rfl, rfl⟩
      obtain ⟨rfl, hC⟩ := hF C₀ s₀ h₀
      exact ⟨by simp [Ty.substT, hp], fun q ⟨b, hb, h'⟩ => by rw [hav b q h'] at hb; exact hC q hb⟩
    · have := Stage.le_substT σ hφ'
      simp only [Stage.substT, Row.union_substT, Row.scope_substT, Ty.substT, hp] at this
      refine Stage.le_rt_sub this ?_
      rintro x (hx | hx | hx | hx)
      · exact .inl hx
      · exact .inr (.inl hx)
      · exact .inr (.inr (.inl (Row.disch_substT (fun y hy => by rw [hp] at hy; exact hy) x hx)))
      · exact .inr (.inr (.inr (Row.needs_substT (fun y hy => by rw [hp] at hy; exact hy) x hx)))
    · obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ _).mp h
      rwa [hav b _ h'] at hb
    · exact Ty.no_rg_substT (t := .errors _) hav hε
  | cap =>
    intro σ hσ
    have hp := hσ.1 _ (.inl rfl)
    simp only [Flick.J.substT, Ty.substT, hp]; exact .cap
  | own =>
    intro σ hσ
    have hp := hσ.1 _ (.inl rfl)
    simp only [Flick.J.substT, Ty.substT, hp]; exact .own
  | drec hc hk _ hn hh ih => exact Typ.drec_substT hc hk ih hn hh
  | dnil => intro _ _; exact .dnil
  | dcons _ _ ih₁ ih₂ => exact Typ.dcons_substT ih₁ ih₂
  | dproj _ hc hk hl hts hH hT ih => exact Typ.dproj_substT ih hc hk hl hts hH hT
  | glob hg hts hH hT => intro σ _; exact Typ.glob_substT hg hts hH hT σ

end Flick
