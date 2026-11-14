import Formal.Cut
import Formal.ContextEquivalence
import Formal.Live.InstanceConversion
import Formal.PatternEquivalence

/-!
# Congruent contexts for unevaluated terms

An arm of a GADT has `NoIns` before instantiation. Its variables and store cells
may be presented by congruent types without changing the judgment. Runtime
handler frames are excluded by that existing premise; continuation values and
locations are transported through the actual type of their store cell.
-/

namespace Flick

variable {S : Sig}

theorem Tm.NoIns.sub {e d : Tm} (h : e.NoIns) (hd : d.Has .ins → e.Has .ins) : d.NoIns :=
  fun h' => h (hd h')

theorem CtxEquiv.prepend {Γ Γ'} (h : CtxEquiv Γ Γ') (Δ : List Scheme) :
    CtxEquiv (Δ ++ Γ) (Δ ++ Γ') := (CtxEquiv.refl Δ).append h

theorem Typ.context_equiv {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ {Ψ' Γ'}, e.NoIns → Ty.ArgsEquiv Ψ Ψ' → CtxEquiv Γ Γ' → Typ S Ψ' Θ Γ' e J := by
  induction h with
  | conv _ he ih => intro _ _ hn hΨ hΓ; exact (ih hn hΨ hΓ).conv he
  | var hi hts hH hT =>
    intro _ _ _ _ hΓ
    obtain ⟨s', hi', he⟩ := hΓ.lookup hi
    exact .var hi' (hts.trans he.arity) (he.holds hH) (hT.equiv_left (he.inst _).symm)
  | allVar hi hk hc =>
    intro _ _ _ _ hΓ
    obtain ⟨s', hi', he⟩ := hΓ.lookup hi
    exact .conv (.allVar hi' (he.arity ▸ hk) (he.no_facts hc)) he.all.symm
  | allI hv _ ih => intro _ _ hn hΨ hΓ; exact .allI hv (ih hn (hΨ.up _) (hΓ.up _))
  | lam _ hb ht ih => intro _ _ hn hΨ hΓ; exact .lam (ih hn hΨ (.cons (.refl _) hΓ)) hb ht
  | app _ _ hφ ht ihf ihx =>
    intro _ _ hn hΨ hΓ
    exact .app (ihf (hn.sub Or.inl) hΨ hΓ) (ihx (hn.sub Or.inr) hΨ hΓ) hφ ht
  | let_ _ _ ihe ihb =>
    intro _ _ hn hΨ hΓ
    exact .let_ (ihe (hn.sub Or.inl) hΨ hΓ) (ihb (hn.sub Or.inr) hΨ (.cons (.refl _) hΓ))
  | letGen _ _ ihe ihb =>
    intro _ _ hn hΨ hΓ
    exact .letGen (ihe (hn.sub Or.inl) (hΨ.up _) (hΓ.up _))
      (ihb (hn.sub Or.inr) hΨ (.cons (.refl _) hΓ))
  | letrec _ hb _ ihf ihb =>
    intro _ _ hn hΨ hΓ
    exact .letrec (ihf (hn.sub Or.inl) hΨ (.cons (.refl _) (.cons (.refl _) hΓ))) hb
      (ihb (hn.sub Or.inr) hΨ (.cons (.refl _) hΓ))
  | fix _ hb ht ih =>
    intro _ _ hn hΨ hΓ
    exact .fix (ih hn hΨ (.cons (.refl _) (.cons (.refl _) hΓ))) hb ht
  | lit => intros; exact .lit
  | loc hl =>
    intro _ _ _ hΨ _
    obtain ⟨t', hl', he⟩ := hΨ.lookup hl
    exact (Typ.loc hl').conv (.ref he.symm)
  | const hc ht => intros; exact .const hc ht
  | ctor hc hk hH ht => intros; exact .ctor hc hk hH ht
  | rnil => intros; exact .rnil
  | rcons hR hl _ _ ihe ihr =>
    intro _ _ hn hΨ hΓ
    exact .rcons hR hl (ihe (hn.sub Or.inl) hΨ hΓ) (ihr (hn.sub Or.inr) hΨ hΓ)
  | proj _ hl ht ih => intro _ _ hn hΨ hΓ; exact .proj (ih hn hΨ hΓ) hl ht
  | update _ hl _ ihe ihx =>
    intro _ _ hn hΨ hΓ
    exact .update (ihe (hn.sub Or.inl) hΨ hΓ) hl (ihx (hn.sub Or.inr) hΨ hΓ)
  | match_ _ _ hc ihe iha =>
    intro _ _ hn hΨ hΓ
    exact .match_ (ihe (hn.sub Or.inl) hΨ hΓ) (iha (hn.sub Or.inr) hΨ hΓ) hc
  | anil => intros; exact .anil
  | acons hp _ _ ihb ihr =>
    intro _ _ hn hΨ hΓ
    exact .acons hp (ihb (hn.sub Or.inl) hΨ (hΓ.prepend _)) (ihr (hn.sub Or.inr) hΨ hΓ)
  | aguard hp _ _ _ ihg ihb ihr =>
    intro _ _ hn hΨ hΓ
    exact .aguard hp (ihg (hn.sub Or.inl) hΨ (hΓ.prepend _))
      (ihb (hn.sub fun h => .inr (.inl h)) hΨ (hΓ.prepend _))
      (ihr (hn.sub fun h => .inr (.inr h)) hΨ hΓ)
  | aref hc hU hb hfr hlc hpA _ _ ihA ihr =>
    intro _ _ hn hΨ hΓ
    exact .aref hc hU hb (fun i hi => by rw [← hΓ.length]; exact hfr i hi)
      (fun l hl => by rw [← hΨ.length]; exact hlc l hl) hpA
      (fun θ hθ hu => ihA θ hθ hu hb ((hΨ.up _).substT θ) ((hΓ.up _).substT θ |>.prepend _))
      (ihr (hn.sub Or.inr) hΨ hΓ)
  | todo => intros; exact .todo
  | task _ hφ hb ht ih => intro _ _ hn hΨ hΓ; exact .task (ih hn hΨ hΓ) hφ hb ht
  | await _ hφ ht ih => intro _ _ hn hΨ hΓ; exact .await (ih hn hΨ hΓ) hφ ht
  | raise _ hφ ih => intro _ _ hn hΨ hΓ; exact .raise (ih hn hΨ hΓ) hφ
  | raiseAdt _ hU hφ ih => intro _ _ hn hΨ hΓ; exact .raiseAdt (ih hn hΨ hΓ) hU hφ
  | errVal hv _ hU hε ih => intro _ _ hn hΨ hΓ; exact .errVal hv (ih hn hΨ hΓ) hU hε
  | tryR _ _ hφ hs ihe iha =>
    intro _ _ hn hΨ hΓ
    exact .tryR (ihe (hn.sub Or.inl) hΨ hΓ) (iha (hn.sub Or.inr) hΨ hΓ) hφ hs
  | tryP _ _ ihe iha =>
    intro _ _ hn hΨ hΓ
    exact .tryP (ihe (hn.sub Or.inl) hΨ hΓ) (iha (hn.sub Or.inr) hΨ hΓ)
  | handle hv _ _ hc ha hφ ihv iha =>
    intro _ _ hn hΨ hΓ
    exact .handle hv (ihv (hn.sub Or.inl) hΨ hΓ) (iha (hn.sub Or.inr) hΨ hΓ) hc ha hφ
  | hnil hH => intros; exact .hnil hH
  | hcons hp _ _ hH ihb ihr =>
    intro _ _ hn hΨ hΓ
    exact .hcons hp (ihb (hn.sub Or.inl) hΨ (hΓ.prepend _)) (ihr (hn.sub Or.inr) hΨ hΓ) hH
  | hguard hp _ _ _ hH ihg ihb ihr =>
    intro _ _ hn hΨ hΓ
    exact .hguard hp (ihg (hn.sub Or.inl) hΨ (hΓ.prepend _))
      (ihb (hn.sub fun h => .inr (.inl h)) hΨ (hΓ.prepend _))
      (ihr (hn.sub fun h => .inr (.inr h)) hΨ hΓ) hH
  | scope hb hF _ hφ' hφ ht ih =>
    intro _ _ hn hΨ hΓ
    exact .scope hb hF (ih hn (hΨ.up _) (.cons (.refl _) (hΓ.up _))) hφ' hφ ht
  | inside => intro _ _ hn _ _; exact (hn (.inr (.inl rfl))).elim
  | cap => intros; exact .cap
  | own => intros; exact .own
  | drec hc hk _ hb ih => intro _ _ hn hΨ hΓ; exact .drec hc hk (ih hn hΨ hΓ) hb
  | dnil => intros; exact .dnil
  | dcons _ _ ihv ihr =>
    intro _ _ hn hΨ hΓ
    exact .dcons (ihv (hn.sub Or.inl) (hΨ.up _) (hΓ.up _)) (ihr (hn.sub Or.inr) hΨ hΓ)
  | dproj _ hc hk hl hts hH ht ih =>
    intro _ _ hn hΨ hΓ; exact .dproj (ih hn hΨ hΓ) hc hk hl hts hH ht
  | glob hg hts hH ht => intros; exact .glob hg hts hH ht
  | perform hop ht => intros; exact .perform hop ht
  | hanil => intros; exact .hanil
  | hatail hop _ _ ihb ihr =>
    intro _ _ hn hΨ hΓ
    exact .hatail hop (ihb (hn.sub Or.inl) hΨ (.cons (.refl _) hΓ)) (ihr (hn.sub Or.inr) hΨ hΓ)
  | haabort hop _ _ ihb ihr =>
    intro _ _ hn hΨ hΓ
    exact .haabort hop (ihb (hn.sub Or.inl) hΨ (.cons (.refl _) hΓ)) (ihr (hn.sub Or.inr) hΨ hΓ)
  | hactl hop _ _ ihb ihr =>
    intro _ _ hn hΨ hΓ
    exact .hactl hop (ihb (hn.sub Or.inl) hΨ (.cons (.refl _) (.cons (.refl _) hΓ)))
      (ihr (hn.sub Or.inr) hΨ hΓ)
  | hdl _ hb _ hψ _ hφ₀ hOb hc hφ ht iha ihr ihe =>
    intro _ _ hn hΨ hΓ
    exact .hdl (iha (hn.sub Or.inl) (hΨ.up _) (hΓ.up _)) hb
      (ihr (hn.sub fun h => .inr (.inl h)) hΨ hΓ) hψ
      (ihe (hn.sub fun h => .inr (.inr h)) hΨ hΓ) hφ₀ hOb hc hφ ht
  | kont hl hp ht =>
    intro _ _ _ hΨ _
    obtain ⟨t', hl', he⟩ := hΨ.lookup hl
    cases he.form with
    | rows hr _ => cases hr
    | arrow _ _ => exact .kont hl' hp ((LeLive.equiv he.symm).trans ht)
  | hwith | hctl | hkeep | hkarm | hres | hdone | unw =>
    intro _ _ hn _ _; exact (hn (.inl rfl)).elim

/-- Equivalent fact presentations entail the same outlives queries. -/
theorem Facts.Equiv.entails {Θ Φ : Facts} (h : Facts.Equiv Θ Φ) : Entails Θ Φ := by
  apply Entails.of_facts
  intro C s hs
  obtain ⟨s₀, h₀, he⟩ := (h C s).mp ⟨s, hs, .refl⟩
  exact (Outl.fact h₀).equiv he

/-- Reusing an unevaluated arm at congruent instances of its surrounding context.
The arm's own bindings `Δ` are kept, as by conversion of completed patterns. -/
theorem Typ.context_instances {Ψ : List Ty} {Θ : Facts} {Γ Δ : List Scheme}
    {e : Tm} {t : Ty} {φ : Stage} {σ τ : Atom → Ty}
    (h : Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Δ ++ Γ.map (Scheme.substT σ)) e
      (.ex (t.substT σ) (φ.substT σ)))
    (hn : e.NoIns) (he : ∀ a, Ty.Equiv (σ a) (τ a)) :
    Typ S (Ψ.map (Ty.substT τ)) (Θ.substT τ) (Δ ++ Γ.map (Scheme.substT τ)) e
      (.ex (t.substT τ) (φ.substT τ)) := by
  have h' := h.cut (Facts.Equiv.of_subst he Θ).entails
  have h'' := h'.context_equiv hn (Ty.ArgsEquiv.of_subst Ψ he) ((CtxEquiv.of_subst Γ he).prepend Δ)
  have hout := h''.conv (t.substT_equiv he)
  rw [Stage.substT_equiv he] at hout
  exact hout

end Flick
