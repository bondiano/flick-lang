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

theorem Entails.trans {Θ₁ Θ₂ Θ₃ : Facts} (h₁ : Entails Θ₁ Θ₂) (h₂ : Entails Θ₂ Θ₃) :
    Entails Θ₁ Θ₃ := fun C s h => h₂ C s (h₁ C s h)

/-- Facts that hold of the order of creation entail themselves under `Facts.age`. -/
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
theorem Typ.cut {Ψ Θ₁ Γ e J} (h : Typ S Ψ Θ₁ Γ e J) : ∀ {Θ₂}, Entails Θ₁ Θ₂ → Typ S Ψ Θ₂ Γ e J := by
  induction h with
  | var hi hts hH hT => intro _ hΘ; exact .var hi hts (hH.mono hΘ) hT
  | lam _ hn hc hT ih => intro _ hΘ; exact .lam (ih hΘ) hn hc hT
  | app _ _ hψ hT ih₁ ih₂ => intro _ hΘ; exact .app (ih₁ hΘ) (ih₂ hΘ) hψ hT
  | let_ _ _ ih₁ ih₂ => intro _ hΘ; exact .let_ (ih₁ hΘ) (ih₂ hΘ)
  | letGen _ _ ih₁ ih₂ => intro _ hΘ; exact .letGen (ih₁ (hΘ.lift _ _)) (ih₂ hΘ)
  | letrec _ hn hc _ ih₁ ih₂ => intro _ hΘ; exact .letrec (ih₁ hΘ) hn hc (ih₂ hΘ)
  | fix _ hn hc hT ih => intro _ hΘ; exact .fix (ih hΘ) hn hc hT
  | lit => intro _ _; exact .lit
  | loc hl => intro _ _; exact .loc hl
  | const hc hT => intro _ hΘ; exact .const (Const.ok_mono hc hΘ) hT
  | ctor hK hk hT => intro _ _; exact .ctor hK hk hT
  | rnil => intro _ _; exact .rnil
  | rcons hR hl _ _ ih₁ ih₂ => intro _ hΘ; exact .rcons hR hl (ih₁ hΘ) (ih₂ hΘ)
  | proj _ hf hT ih => intro _ hΘ; exact .proj (ih hΘ) hf hT
  | update _ hf _ ih₁ ih₂ => intro _ hΘ; exact .update (ih₁ hΘ) hf (ih₂ hΘ)
  | match_ _ _ hc ih₁ ih₂ => intro _ hΘ; exact .match_ (ih₁ hΘ) (ih₂ hΘ) hc
  | anil => intro _ _; exact .anil
  | acons hp _ _ ih₁ ih₂ => intro _ hΘ; exact .acons hp (ih₁ hΘ) (ih₂ hΘ)
  | aguard hp _ _ _ ih₁ ih₂ ih₃ => intro _ hΘ; exact .aguard hp (ih₁ hΘ) (ih₂ hΘ) (ih₃ hΘ)
  | todo => intro _ _; exact .todo
  | task _ hψ hn hc hT ih => intro _ hΘ; exact .task (ih hΘ) hψ hn hc hT
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
  | provideL _ _ h₁ h₂ ih₁ ih₂ => intro _ hΘ; exact .provideL (ih₁ hΘ) (ih₂ hΘ) h₁ h₂
  | provideE _ _ h₁ h₂ ih₁ ih₂ => intro _ hΘ; exact .provideE (ih₁ hΘ) (ih₂ hΘ) h₁ h₂
  | with_ hv _ _ h₁ h₂ ih₁ ih₂ => intro _ hΘ; exact .with_ hv (ih₁ hΘ) (ih₂ hΘ) h₁ h₂
  | enil hρ => intro _ _; exact .enil hρ
  | econs hv hθ _ _ hρ ih₁ ih₂ => intro _ hΘ; exact .econs hv hθ (ih₁ hΘ) (ih₂ hΘ) hρ
  | bind _ hρ ih => intro _ hΘ; exact .bind (ih hΘ) hρ
  | join _ _ hρ ih₁ ih₂ => intro _ hΘ; exact .join (ih₁ hΘ) (ih₂ hΘ) hρ
  | scope hn hF _ hφ' hφ hT ih => intro _ hΘ; exact .scope hn hF (ih (hΘ.lift _ _)) hφ' hφ hT
  | inside hF _ hφ' ht hε hφ hT ih =>
    intro _ hΘ; exact .inside hF (ih (hΘ.union _)) hφ' ht hε hφ hT
  | cap => intro _ _; exact .cap
  | own => intro _ _; exact .own

end Flick
