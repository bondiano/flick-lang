import Formal.SubAdm

/-!
# Type substitution preserves typing

`Γ;Ψ ⊢ e : τ ! φ` implies `Γ[ρ];Ψ[ρ] ⊢ e : τ[ρ] ! φ` for any substitution `ρ`
of type variables. Terms carry no types, so `e` does not change.
-/

namespace Flick

variable {S : Sig}

def J.substT (ρ : Nat → Ty) : J → J
  | .ex t φ => .ex (t.substT ρ) φ
  | .alts t t' φ => .alts (t.substT ρ) (t'.substT ρ) φ
  | .halts ε H t φ => .halts ε H (t.substT ρ) φ

theorem Le.substT {a b : Ty} (ρ : Nat → Ty) (h : Le a b) : Le (a.substT ρ) (b.substT ρ) := by
  induction h with
  | refl => exact .refl
  | arrow _ hφ _ ih₁ ih₂ => exact .arrow ih₁ hφ ih₂
  | eff hr he _ ih => exact .eff hr he ih

theorem Shp.of_substT {ρ : Nat → Ty} : ∀ {t : Ty} {v : Tm}, Shp S true (t.substT ρ) v → Shp S true t v
  | .tvar _, _, _ | .arrow .., _, _ | .ref _, _, _ | .eff .., _, _ | .layer .., _, _
  | .errors _, _, _ | .env _, _, _ => .hidden trivial
  | .base _, _, h | .adt _, _, h | .rnil, _, h => h
  | .rcons _ _ _, _, h => by
    simp only [Ty.substT] at h
    cases h with
    | hidden ho => exact ho.elim
    | rcons h₁ h₂ => exact .rcons (Shp.of_substT h₁) (Shp.of_substT h₂)

theorem Covers.substT {t ps e} (ρ : Nat → Ty) (h : Covers S t ps e) : Covers S (t.substT ρ) ps e :=
  h.imp (fun hx v hs => hx v (Shp.of_substT hs)) id

theorem PatTy.substT {fl p t Δ} (ρ : Nat → Ty) (h : PatTy S fl p t Δ) :
    PatTy S fl p (t.substT ρ) (Δ.map (Ty.substT ρ)) := by
  induction h with
  | wild => exact .wild
  | var => exact .var
  | lit => exact .lit
  | head hK => rw [S.ctor_ty_substT ρ hK]; exact .head hK
  | app _ _ ih₁ ih₂ => rw [List.map_append]; exact .app ih₁ ih₂
  | adt _ ih => exact .adt ih
  | rany => exact .rany
  | rfield hR _ _ ih₁ ih₂ => rw [List.map_append]; exact .rfield (Ty.field_substT ρ hR) ih₁ ih₂
  | or _ _ ih₁ ih₂ => exact .or ih₁ ih₂

theorem HPat.substT {ε₀ p θ Δ} (ρ : Nat → Ty) (h : HPat S ε₀ p θ Δ) :
    θ.substT ρ = θ ∧ HPat S ε₀ p θ (Δ.map (Ty.substT ρ)) := by
  cases h with
  | wild => exact ⟨rfl, .wild⟩
  | var => exact ⟨rfl, .var⟩
  | ctor hp => exact ⟨rfl, .ctor (hp.substT ρ)⟩

theorem ctx_substT (Δ : List Ty) (Γ : List Scheme) (ρ : Nat → Ty) :
    (Δ.map Scheme.mono ++ Γ).map (Scheme.substT ρ) =
      (Δ.map (Ty.substT ρ)).map Scheme.mono ++ Γ.map (Scheme.substT ρ) := by
  simp

theorem shift_comm (Ψ : List Ty) (k : Nat) (ρ : Nat → Ty) :
    (Ψ.map (Ty.up k)).map (Ty.substT (liftT k ρ)) = (Ψ.map (Ty.substT ρ)).map (Ty.up k) := by
  simp [Ty.up_comm]

theorem shift_comm' (Γ : List Scheme) (k : Nat) (ρ : Nat → Ty) :
    (Γ.map (Scheme.up k)).map (Scheme.substT (liftT k ρ)) =
      (Γ.map (Scheme.substT ρ)).map (Scheme.up k) := by
  simp [Scheme.up_comm]

theorem Typ.letGen_substT {Ψ : List Ty} {Γ : List Scheme} {e b : Tm} {a t : Ty} {φ : Stage}
    {k : Nat}
    (ih₁ : ∀ ρ, Typ S ((Ψ.map (Ty.up k)).map (Ty.substT ρ)) ((Γ.map (Scheme.up k)).map
      (Scheme.substT ρ)) e (J.substT ρ (.ex a .pure)))
    (ih₂ : ∀ ρ, Typ S (Ψ.map (Ty.substT ρ)) ((⟨k, a⟩ :: Γ).map (Scheme.substT ρ)) b
      (J.substT ρ (.ex t φ))) (ρ : Nat → Ty) :
    Typ S (Ψ.map (Ty.substT ρ)) (Γ.map (Scheme.substT ρ)) (.let_ e b) (J.substT ρ (.ex t φ)) := by
  have h₁ := ih₁ (liftT k ρ)
  rw [shift_comm, shift_comm'] at h₁
  exact .letGen h₁ (ih₂ ρ)

theorem Typ.substT {Ψ Γ e J} (h : Typ S Ψ Γ e J) :
    ∀ ρ, Typ S (Ψ.map (Ty.substT ρ)) (Γ.map (Scheme.substT ρ)) e (J.substT ρ) := by
  induction h with
  | @var _ _ σ _ _ _ ts hi hts hT =>
    intro ρ
    refine .var (σ := σ.substT ρ) (ts := ts.map (Ty.substT ρ)) (by simp [hi]) (by simp [Scheme.substT, hts]) ?_
    rw [Scheme.inst_substT _ _ _ hts]; exact hT.substT ρ
  | lam _ hT ih =>
    intro ρ; have := ih ρ; simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at this
    exact .lam this (hT.substT ρ)
  | app _ _ hψ hT ih₁ ih₂ => intro ρ; exact .app (ih₁ ρ) (ih₂ ρ) hψ (hT.substT ρ)
  | let_ _ _ ih₁ ih₂ =>
    intro ρ; have := ih₂ ρ; simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at this
    exact .let_ (ih₁ ρ) this
  | letGen _ _ ih₁ ih₂ => exact Typ.letGen_substT ih₁ ih₂
  | letrec _ _ ih₁ ih₂ =>
    intro ρ
    have h₁ := ih₁ ρ; simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at h₁
    have h₂ := ih₂ ρ; simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at h₂
    exact .letrec h₁ h₂
  | fix _ hT ih =>
    intro ρ; have := ih ρ; simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at this
    exact .fix this (hT.substT ρ)
  | lit => intro _; exact .lit
  | loc hl => intro ρ; exact .loc (by simp [hl])
  | const hc hT =>
    intro ρ; exact .const hc (by have := hT.substT ρ; rwa [Const.ty_substT] at this)
  | ctor hK hT =>
    intro ρ; exact .ctor hK (by have := hT.substT ρ; rwa [S.ctor_ty_substT ρ hK] at this)
  | rnil => intro _; exact .rnil
  | rcons hR hl _ _ ih₁ ih₂ =>
    intro ρ
    exact .rcons (hR.substT ρ) (by rw [Ty.labels_substT ρ hR]; exact hl) (ih₁ ρ) (ih₂ ρ)
  | proj _ hf hT ih => intro ρ; exact .proj (ih ρ) (Ty.field_substT ρ hf) (hT.substT ρ)
  | update _ hf _ ih₁ ih₂ => intro ρ; exact .update (ih₁ ρ) (Ty.field_substT ρ hf) (ih₂ ρ)
  | match_ _ _ hc ih₁ ih₂ => intro ρ; exact .match_ (ih₁ ρ) (ih₂ ρ) (hc.substT ρ)
  | anil => intro _; exact .anil
  | acons hp _ _ ih₁ ih₂ =>
    intro ρ
    have := ih₁ ρ; rw [ctx_substT] at this
    exact .acons (hp.substT ρ) this (ih₂ ρ)
  | aguard hp _ _ _ ih₁ ih₂ ih₃ =>
    intro ρ
    have g := ih₁ ρ; rw [ctx_substT] at g
    have b := ih₂ ρ; rw [ctx_substT] at b
    exact .aguard (hp.substT ρ) g b (ih₃ ρ)
  | todo => intro _; exact .todo
  | task _ hψ hT ih => intro ρ; exact .task (ih ρ) hψ (hT.substT ρ)
  | await _ hφ hT ih => intro ρ; exact .await (ih ρ) hφ (hT.substT ρ)
  | raise _ hφ ih => intro ρ; exact .raise (ih ρ) hφ
  | raiseAdt _ hφ ih => intro ρ; exact .raiseAdt (ih ρ) hφ
  | errVal hv _ hE ih => intro ρ; exact .errVal hv (ih ρ) hE
  | tryR _ _ hφ hs ih₁ ih₂ => intro ρ; exact .tryR (ih₁ ρ) (ih₂ ρ) hφ hs
  | tryP _ _ ih₁ ih₂ => intro ρ; exact .tryP (ih₁ ρ) (ih₂ ρ)
  | handle hv _ _ hc hφ ih₁ ih₂ => intro ρ; exact .handle hv (ih₁ ρ) (ih₂ ρ) hc hφ
  | hnil hH => intro _; exact .hnil hH
  | hcons hp _ _ hH ih₁ ih₂ =>
    intro ρ
    obtain ⟨hθ, hp'⟩ := hp.substT ρ
    have := ih₁ ρ; rw [ctx_substT] at this
    exact .hcons hp' this (ih₂ ρ) hH
  | hguard hp _ _ _ hH ih₁ ih₂ ih₃ =>
    intro ρ
    obtain ⟨_, hp'⟩ := hp.substT ρ
    have g := ih₁ ρ; rw [ctx_substT] at g
    have b := ih₂ ρ; rw [ctx_substT] at b
    exact .hguard hp' g b (ih₃ ρ) hH
  | provideL _ _ h₁ h₂ ih₁ ih₂ => intro ρ; exact .provideL (ih₁ ρ) (ih₂ ρ) h₁ h₂
  | provideE _ _ h₁ h₂ ih₁ ih₂ => intro ρ; exact .provideE (ih₁ ρ) (ih₂ ρ) h₁ h₂
  | with_ hv _ _ h₁ h₂ ih₁ ih₂ => intro ρ; exact .with_ hv (ih₁ ρ) (ih₂ ρ) h₁ h₂
  | enil hρ => intro _; exact .enil hρ
  | econs hv hθ _ _ hρ ih₁ ih₂ =>
    intro ρ
    have := ih₁ ρ; simp only [Flick.J.substT, S.impl_closed] at this
    exact .econs hv hθ this (ih₂ ρ) hρ
  | bind _ hρ ih =>
    intro ρ
    have := ih ρ; simp only [Flick.J.substT, S.impl_closed] at this
    exact .bind this hρ
  | join _ _ hρ ih₁ ih₂ => intro ρ; exact .join (ih₁ ρ) (ih₂ ρ) hρ

end Flick
