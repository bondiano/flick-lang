import Formal.Weaken

/-!
# Facts that follow from the context may be dropped

`Θ ⊢ Outlives C ϱ` is used only by the premises of (Var) and (Const), and is
monotone in `Θ`: a judgment under `Θ₁` holds under any `Θ₂` that gives every
`Outlives` `Θ₁` gives. (Let-Gen) assumes the facts of its scheme, and the
substitution lemma drops them again with the facts the use of the variable checks.
-/

namespace Flick

variable {S : Sig}

/-- `Θ₁` gives no more than `Θ₂`. -/
def Entails (Θ₁ Θ₂ : Facts) : Prop := ∀ C s, Outl Θ₁ C s → Outl Θ₂ C s

theorem Entails.refl (Θ : Facts) : Entails Θ Θ := fun _ _ h => h

theorem Entails.lift {Θ₁ Θ₂ : Facts} (h : Entails Θ₁ Θ₂) (k : Nat) (Φ : Facts) :
    Entails (Facts.union (Θ₁.substT (shiftV k)) Φ) (Facts.union (Θ₂.substT (shiftV k)) Φ) := by
  intro C s hC x hx
  rcases hC x hx with h' | ⟨C', h₁ | h₁, h₂⟩
  · exact .inl h'
  · obtain ⟨C₀, s₀, h₀, rfl, rfl⟩ := h₁
    have := (h C₀ s₀ fun y hy => .inr ⟨C₀, h₀, hy⟩).substT (shiftV k) x h₂
    rcases this with h' | ⟨C'', h₃, h₄⟩
    · exact .inl h'
    · exact .inr ⟨C'', .inl h₃, h₄⟩
  · exact .inr ⟨C', .inr h₁, h₂⟩

theorem Entails.union {Θ₁ Θ₂ : Facts} (h : Entails Θ₁ Θ₂) (F : Facts) :
    Entails (Facts.union Θ₁ F) (Facts.union Θ₂ F) := by
  intro C s hC x hx
  rcases hC x hx with h' | ⟨C', h₁ | h₁, h₂⟩
  · exact .inl h'
  · rcases h C' s (fun y hy => .inr ⟨C', h₁, hy⟩) x h₂ with h' | ⟨C'', h₃, h₄⟩
    · exact .inl h'
    · exact .inr ⟨C'', .inl h₃, h₄⟩
  · exact .inr ⟨C', .inr h₁, h₂⟩

theorem Entails.substT {Θ₁ Θ₂ : Facts} (h : Entails Θ₁ Θ₂) (σ : Atom → Ty) :
    Entails (Θ₁.substT σ) (Θ₂.substT σ) := by
  intro C s hC x hx
  rcases hC x hx with h' | ⟨C', ⟨C₀, s₀, h₀, rfl, rfl⟩, h₂⟩
  · exact .inl h'
  · exact (h C₀ s₀ fun y hy => .inr ⟨C₀, h₀, hy⟩).substT σ x h₂

theorem Entails.trans {Θ₁ Θ₂ Θ₃ : Facts} (h₁ : Entails Θ₁ Θ₂) (h₂ : Entails Θ₂ Θ₃) :
    Entails Θ₁ Θ₃ := fun C s h => h₂ C s (h₁ C s h)

/-- Facts that hold of the order of creation entail themselves under `Facts.age`. -/
theorem Entails.union_none {Θ : Facts} : Entails (Facts.union Θ Facts.none) Θ :=
  fun _ _ h x hx => (h x hx).imp_right fun ⟨C', hu, h₂⟩ => ⟨C', hu.elim id False.elim, h₂⟩

theorem Entails.of_age {Θ : Facts} (h : ∀ C s, Θ C s → Facts.age C s) : Entails Θ Facts.age :=
  fun _ _ hC x hx => (hC x hx).imp id fun ⟨C', h₁, h₂⟩ => ⟨C', h _ _ h₁, h₂⟩

theorem Entails.none (Θ : Facts) : Entails Facts.none Θ :=
  fun _ _ h x hx => (h x hx).imp id fun ⟨_, h, _⟩ => h.elim

/-- `Θ'` is `Θ` with facts that hold of the order in which regions are created:
the facts the `inside` frames of a context add. -/
def Ext (Θ Θ' : Facts) : Prop :=
  (∀ C s, Θ C s → Θ' C s) ∧ ∀ C s, Θ' C s → Θ C s ∨ Facts.age C s

theorem Ext.refl (Θ : Facts) : Ext Θ Θ := ⟨fun _ _ h => h, fun _ _ h => .inl h⟩

theorem Ext.trans {Θ₁ Θ₂ Θ₃ : Facts} (h₁ : Ext Θ₁ Θ₂) (h₂ : Ext Θ₂ Θ₃) : Ext Θ₁ Θ₃ :=
  ⟨fun C s h => h₂.1 C s (h₁.1 C s h), fun C s h => (h₂.2 C s h).elim (h₁.2 C s) .inr⟩

theorem Ext.inside {Θ F : Facts} {p : Nat} (hF : ∀ C s, F C s → s = .rg p ∧ ∀ q, C (.r q) → q < p) :
    Ext Θ (Facts.union Θ F) :=
  ⟨fun _ _ h => .inl h, fun C s h => h.elim .inl fun h => .inr fun q p' hq hp' => by
    obtain ⟨rfl, hC⟩ := hF C s h
    cases hp'; exact hC q hq⟩

theorem Entails.union_left (Θ Φ : Facts) : Entails Θ (Facts.union Θ Φ) :=
  fun _ _ h => h.mono fun _ _ h => .inl h

theorem Scheme.Holds.mono {Θ₁ Θ₂ : Facts} {s : Scheme} {ts : List Ty} (h : s.Holds Θ₁ ts)
    (hΘ : Entails Θ₁ Θ₂) : s.Holds Θ₂ ts :=
  fun C r hc => hΘ _ _ (h C r hc)

/-- Facts that follow from the context may be dropped. -/
theorem Row.hP_mono {Θ₁ Θ₂ : Facts} {q : Nat} {E : Name} {r : Row} (hΘ : Entails Θ₁ Θ₂) :
    Row.Sub (Row.hP Θ₁ q E r) (Row.hP Θ₂ q E r) := by
  intro x hx
  cases x with
  | ob N a => exact ⟨fun h => hΘ _ _ (hx.1 h), hx.2⟩
  | _ => exact hx

theorem Typ.cut {Ψ Θ₁ Γ e J} (h : Typ S Ψ Θ₁ Γ e J) : ∀ {Θ₂}, Entails Θ₁ Θ₂ → Typ S Ψ Θ₂ Γ e J := by
  induction h with
  | var hi hts hH hT => intro _ hΘ; exact .var hi hts (hH.mono hΘ) hT
  | allVar hi hk hc => intro _ _; exact .allVar hi hk hc
  | allI hv _ ih => intro _ hΘ; exact .allI hv (ih (hΘ.substT _))
  | lam _ hn hT ih => intro _ hΘ; exact .lam (ih hΘ) hn hT
  | app _ _ hψ hT ih₁ ih₂ => intro _ hΘ; exact .app (ih₁ hΘ) (ih₂ hΘ) hψ hT
  | let_ _ _ ih₁ ih₂ => intro _ hΘ; exact .let_ (ih₁ hΘ) (ih₂ hΘ)
  | letGen _ _ ih₁ ih₂ => intro _ hΘ; exact .letGen (ih₁ (hΘ.lift _ _)) (ih₂ hΘ)
  | letrec _ hn _ ih₁ ih₂ => intro _ hΘ; exact .letrec (ih₁ hΘ) hn (ih₂ hΘ)
  | fix _ hn hT ih => intro _ hΘ; exact .fix (ih hΘ) hn hT
  | lit => intro _ _; exact .lit
  | loc hl => intro _ _; exact .loc hl
  | const hc hT => intro _ hΘ; exact .const (Const.ok_mono hc hΘ) hT
  | ctor hK hk hH hT => intro _ _; exact .ctor hK hk hH hT
  | rnil => intro _ _; exact .rnil
  | rcons hR hl _ _ ih₁ ih₂ => intro _ hΘ; exact .rcons hR hl (ih₁ hΘ) (ih₂ hΘ)
  | proj _ hf hT ih => intro _ hΘ; exact .proj (ih hΘ) hf hT
  | update _ hf _ ih₁ ih₂ => intro _ hΘ; exact .update (ih₁ hΘ) hf (ih₂ hΘ)
  | match_ _ _ hc ih₁ ih₂ => intro _ hΘ; exact .match_ (ih₁ hΘ) (ih₂ hΘ) hc
  | anil => intro _ _; exact .anil
  | acons hp _ _ ih₁ ih₂ => intro _ hΘ; exact .acons hp (ih₁ hΘ) (ih₂ hΘ)
  | aguard hp _ _ _ ih₁ ih₂ ih₃ => intro _ hΘ; exact .aguard hp (ih₁ hΘ) (ih₂ hΘ) (ih₃ hΘ)
  | aref hK hU hn hfr hlc hpA _ _ ihA ihr =>
    intro _ hΘ
    exact .aref hK hU hn hfr hlc hpA (fun θ hθ hu => ihA θ hθ hu ((hΘ.substT _).substT θ)) (ihr hΘ)
  | todo => intro _ _; exact .todo
  | task _ hψ hn hT ih => intro _ hΘ; exact .task (ih hΘ) hψ hn hT
  | await _ hφ hT ih => intro _ hΘ; exact .await (ih hΘ) hφ hT
  | raise _ hφ ih => intro _ hΘ; exact .raise (ih hΘ) hφ
  | raiseAdt _ hU hφ ih => intro _ hΘ; exact .raiseAdt (ih hΘ) hU hφ
  | errVal hv _ hU hE ih => intro _ hΘ; exact .errVal hv (ih hΘ) hU hE
  | tryR _ _ hφ hs ih₁ ih₂ => intro _ hΘ; exact .tryR (ih₁ hΘ) (ih₂ hΘ) hφ hs
  | tryP _ _ ih₁ ih₂ => intro _ hΘ; exact .tryP (ih₁ hΘ) (ih₂ hΘ)
  | handle hv _ _ hc ha hφ ih₁ ih₂ => intro _ hΘ; exact .handle hv (ih₁ hΘ) (ih₂ hΘ) hc ha hφ
  | hnil hH => intro _ _; exact .hnil hH
  | hcons hp _ _ hH ih₁ ih₂ => intro _ hΘ; exact .hcons hp (ih₁ hΘ) (ih₂ hΘ) hH
  | hguard hp _ _ _ hH ih₁ ih₂ ih₃ => intro _ hΘ; exact .hguard hp (ih₁ hΘ) (ih₂ hΘ) (ih₃ hΘ) hH
  | scope hn hF _ hφ' hφ hT ih => intro _ hΘ; exact .scope hn hF (ih (hΘ.lift _ _)) hφ' hφ hT
  | inside hF _ hφ' ht hε hφ hT ih =>
    intro _ hΘ; exact .inside hF (ih (hΘ.union _)) hφ' ht hε hφ hT
  | cap => intro _ _; exact .cap
  | own => intro _ _; exact .own
  | drec hc hk _ hn ih => intro _ hΘ; exact .drec hc hk (ih hΘ) hn
  | dnil => intro _ _; exact .dnil
  | dcons _ _ ih₁ ih₂ => intro _ hΘ; exact .dcons (ih₁ (hΘ.lift _ _)) (ih₂ hΘ)
  | dproj _ hc hk hl hts hH hT ih => intro _ hΘ; exact .dproj (ih hΘ) hc hk hl hts (hH.mono hΘ) hT
  | glob hg hts hH hT => intro _ hΘ; exact .glob hg hts (hH.mono hΘ) hT
  | perform hop hT => intro _ _; exact .perform hop hT
  | hanil => intro _ _; exact .hanil
  | hatail hop _ _ ih₁ ih₂ => intro _ hΘ; exact .hatail hop (ih₁ hΘ) (ih₂ hΘ)
  | haabort hop _ _ ih₁ ih₂ => intro _ hΘ; exact .haabort hop (ih₁ hΘ) (ih₂ hΘ)
  | hactl hop _ _ ih₁ ih₂ => intro _ hΘ; exact .hactl hop (ih₁ hΘ) (ih₂ hΘ)
  | hdl _ hn _ hψ _ hφ₀ hOb hc hφ hT ih₁ ih₂ ih₃ =>
    intro _ hΘ; exact .hdl (ih₁ (hΘ.substT _)) hn (ih₂ hΘ) hψ (ih₃ hΘ) hφ₀ hOb hc hφ hT
  | hwith hℓ _ _ hψ _ hφ₀ hc hφ hT ha ih₁ ih₂ ih₃ =>
    intro _ hΘ; exact .hwith hℓ (ih₁ hΘ) (ih₂ hΘ) hψ (ih₃ hΘ) (Stage.le_rt_sub hφ₀ (Row.hP_mono hΘ)) hc hφ hT ha
  | hctl hℓ _ _ hψ hop hc htl hE hk _ hφ₀ _ hl hcm hφ hT ha ih₁ ih₂ ih₃ ih₄ =>
    intro _ hΘ; exact .hctl hℓ (ih₁ hΘ) (ih₂ hΘ) hψ hop hc htl hE hk (ih₃ hΘ) (Stage.le_rt_sub hφ₀ (Row.hP_mono hΘ)) (ih₄ hΘ) hl hcm hφ hT ha
  | hkeep hℓ _ _ hφ₀ hc hnt hle hφ hT ih₁ ih₂ =>
    intro _ hΘ; exact .hkeep hℓ (ih₁ hΘ) (ih₂ hΘ) (Stage.le_rt_sub hφ₀ (Row.hP_mono hΘ)) hc hnt hle hφ hT
  | hkarm hℓ _ hop hE hk _ hφ₀ hc hnt _ hle hφ hT ih₁ ih₂ ih₃ =>
    intro _ hΘ; exact .hkarm hℓ (ih₁ hΘ) hop hE hk (ih₂ hΘ) (Stage.le_rt_sub hφ₀ (Row.hP_mono hΘ)) hc hnt (ih₃ hΘ) hle hφ hT
  | hres hℓ _ hA ha _ hl hcp hφ hT ih₁ ih₂ => intro _ hΘ; exact .hres hℓ (ih₁ hΘ) hA ha (ih₂ hΘ) hl hcp hφ hT
  | hdone _ ih => intro _ hΘ; exact .hdone (ih hΘ)
  | unw hℓ _ hφ ih => intro _ hΘ; exact .unw hℓ (ih hΘ) hφ
  | kont hl hC hT => intro _ _; exact .kont hl hC hT

end Flick
