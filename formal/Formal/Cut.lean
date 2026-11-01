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
  | inside _ hφ' hφ hT ih => intro _ hΘ; exact .inside (ih hΘ) hφ' hφ hT
  | cap => intro _ _; exact .cap
  | own => intro _ _; exact .own

end Flick
