import Formal.Weaken
import Formal.Order

/-!
# (Sub) is admissible (core.md §2.3)

Raising the result type along `≤` and the stage along `⊑` keeps a derivation.
-/

namespace Flick

variable {S : Sig}

/-- Subsumption between judgment forms. -/
def JLe : J → J → Prop
  | .ex t φ, .ex t' φ' => Le t t' ∧ φ.le φ'
  | .alts t r φ, .alts t' r' φ' => t = t' ∧ Le r r' ∧ φ.le φ'
  | .halts ε H r φ, .halts ε' H' r' φ' => ε = ε' ∧ H = H' ∧ Le r r' ∧ φ.le φ'
  | .dfs fs, .dfs fs' => fs = fs'
  | .harms E t φ s, .harms E' t' φ' s' => E = E' ∧ t = t' ∧ φ = φ' ∧ s = s'
  | _, _ => False

theorem JLe.ex {t φ J'} (h : JLe (.ex t φ) J') :
    ∃ t' φ', J' = .ex t' φ' ∧ Le t t' ∧ φ.le φ' := by
  cases J' <;> simp only [JLe] at h
  exact ⟨_, _, rfl, h⟩

theorem JLe.alts {t r φ J'} (h : JLe (.alts t r φ) J') :
    ∃ r' φ', J' = .alts t r' φ' ∧ Le r r' ∧ φ.le φ' := by
  cases J' <;> simp only [JLe] at h
  obtain ⟨rfl, h⟩ := h
  exact ⟨_, _, rfl, h⟩

theorem JLe.halts {ε H r φ J'} (h : JLe (.halts ε H r φ) J') :
    ∃ r' φ', J' = .halts ε H r' φ' ∧ Le r r' ∧ φ.le φ' := by
  cases J' <;> simp only [JLe] at h
  obtain ⟨rfl, rfl, h⟩ := h
  exact ⟨_, _, rfl, h⟩

theorem JLe.dfs {fs J'} (h : JLe (.dfs fs) J') : J' = .dfs fs := by
  cases J' <;> simp only [JLe] at h
  rw [h]

theorem JLe.st {t φ φ'} (h : φ.le φ') : JLe (.ex t φ) (.ex t φ') := ⟨.refl, h⟩

theorem Ty.field_rcons {R : Ty} {l t} (h : R.field l = some t) :
    ∃ l' t' R', R = .rcons l' t' R' := by
  cases R <;> simp [Ty.field] at h
  exact ⟨_, _, _, rfl⟩

theorem Le.substT {a b : Ty} (σ : Atom → Ty) (h : Le a b) : Le (a.substT σ) (b.substT σ) := by
  have sub : ∀ {x y : Ty}, Caps.Sub x.reach y.reach →
      Caps.Sub (x.substT σ).reach (y.substT σ).reach := fun hc a ha => by
    obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ a).mp ha
    exact (Ty.reach_substT σ _ a).mpr ⟨b, hc b hb, h'⟩
  induction h with
  | refl => exact .refl
  | arrow _ hφ _ hc ih₁ ih₂ => exact .arrow ih₁ (Stage.le_substT σ hφ) ih₂ (sub hc)
  | eff hr he _ hc ih => exact .eff (hr.substT σ) (he.substT σ) ih (sub hc)

theorem Typ.sub {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) : ∀ {J'}, JLe J J' → Typ S Ψ Θ Γ e J' := by
  induction h with
  | var hi hts hH hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; exact .var hi hts hH (hT.trans (·.trans h₁))
  | allVar hi hk hc => intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; cases h₁; exact .allVar hi hk hc
  | allI hv he => intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; cases h₁; exact .allI hv he
  | lam hb hn hc hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; exact .lam hb hn hc (hT.trans h₁)
  | app _ _ hψ hT ih₁ ih₂ =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .app (ih₁ (.st h₂)) (ih₂ (.st h₂)) (Stage.le_trans hψ h₂) (hT.trans h₁)
  | let_ _ _ ih₁ ih₂ =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .let_ (ih₁ (.st h₂)) (ih₂ ⟨h₁, h₂⟩)
  | letGen he _ _ ih₂ =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .letGen he (ih₂ ⟨h₁, h₂⟩)
  | letrec hf hn hc _ _ ih₂ =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .letrec hf hn hc (ih₂ ⟨h₁, h₂⟩)
  | fix hf hn hc hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; exact .fix hf hn hc (hT.trans h₁)
  | lit => intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; cases h₁; exact .lit
  | loc hl => intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; cases h₁; exact .loc hl
  | const hc hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; exact .const hc (hT.trans h₁)
  | ctor hK hk hH hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; exact .ctor hK hk hH (hT.trans h₁)
  | rnil => intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; cases h₁; exact .rnil
  | rcons hR hl _ _ ih₁ ih₂ =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex; cases h₁
    exact .rcons hR hl (ih₁ (.st h₂)) (ih₂ (.st h₂))
  | proj _ hf hT ih =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .proj (ih (.st h₂)) hf (hT.trans h₁)
  | update _ hf _ ih₁ ih₂ =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    obtain ⟨_, _, _, rfl⟩ := Ty.field_rcons hf
    cases h₁
    exact .update (ih₁ (.st h₂)) hf (ih₂ (.st h₂))
  | match_ _ _ hc ih₁ ih₂ =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .match_ (ih₁ (.st h₂)) (ih₂ ⟨rfl, h₁, h₂⟩) hc
  | anil => intro _ hJ; obtain ⟨_, _, rfl, _, _⟩ := hJ.alts; exact .anil
  | acons hp _ _ ih₁ ih₂ =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.alts
    exact .acons hp (ih₁ ⟨h₁, h₂⟩) (ih₂ ⟨rfl, h₁, h₂⟩)
  | aguard hp _ _ _ ih₁ ih₂ ih₃ =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.alts
    exact .aguard hp (ih₁ (.st h₂)) (ih₂ ⟨h₁, h₂⟩) (ih₃ ⟨rfl, h₁, h₂⟩)
  | aref hK hU hn hfr hlc hpA _ _ ihA ihr =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.alts
    exact .aref hK hU hn hfr hlc hpA
      (fun θ hθ hu => ihA θ hθ hu ⟨by
        simp only [Ty.up_def]; exact (h₁.substT _).substT θ, Stage.le_substT θ (Stage.le_substT _ h₂)⟩)
      (ihr ⟨rfl, h₁, h₂⟩)
  | todo => intro _ hJ; obtain ⟨_, _, rfl, _, _⟩ := hJ.ex; exact .todo
  | task he hψ hn hc hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; exact .task he hψ hn hc (hT.trans h₁)
  | await _ hφ hT ih =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .await (ih (.st h₂)) (Stage.le_trans hφ h₂) (hT.trans h₁)
  | raise _ hφ ih =>
    intro _ hJ; obtain ⟨_, _, rfl, _, h₂⟩ := hJ.ex
    exact .raise (ih (.st h₂)) (Stage.le_trans hφ h₂)
  | raiseAdt _ hU hφ ih =>
    intro _ hJ; obtain ⟨_, _, rfl, _, h₂⟩ := hJ.ex
    exact .raiseAdt (ih (.st h₂)) hU (Stage.le_trans hφ h₂)
  | errVal hv _ hU hE ih =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex; cases h₁
    exact .errVal hv (ih (.st h₂)) hU hE
  | tryR _ _ hφ hs ih₁ ih₂ =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .tryR (ih₁ ⟨h₁, Stage.le_refl _⟩) (ih₂ ⟨rfl, rfl, h₁, h₂⟩) (Stage.le_trans hφ h₂) hs
  | tryP _ _ ih₁ ih₂ =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .tryP (ih₁ ⟨h₁, Stage.le_refl _⟩) (ih₂ ⟨rfl, rfl, h₁, h₂⟩)
  | handle hv hv' _ hc ha hφ _ ih₂ =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .handle hv hv' (ih₂ ⟨rfl, rfl, h₁, h₂⟩) hc ha (Stage.le_trans hφ h₂)
  | hnil hH => intro _ hJ; obtain ⟨_, _, rfl, _, _⟩ := hJ.halts; exact .hnil hH
  | hcons hp _ _ hH ih₁ ih₂ =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.halts
    exact .hcons hp (ih₁ ⟨h₁, h₂⟩) (ih₂ ⟨rfl, rfl, h₁, h₂⟩) hH
  | hguard hp _ _ _ hH ih₁ ih₂ ih₃ =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.halts
    exact .hguard hp (ih₁ (.st h₂)) (ih₂ ⟨h₁, h₂⟩) (ih₃ ⟨rfl, rfl, h₁, h₂⟩) hH
  | scope hn hF hb hφ' hφ hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .scope hn hF hb hφ' (Stage.le_trans hφ h₂) (hT.trans h₁)
  | inside hF hb hφ' ht hε hφ hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .inside hF hb hφ' ht hε (Stage.le_trans hφ h₂) (hT.trans h₁)
  | cap => intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; cases h₁; exact .cap
  | own => intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; cases h₁; exact .own
  | drec hc hk hr hn hh =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; cases h₁; exact .drec hc hk hr hn hh
  | dnil => intro _ hJ; rw [hJ.dfs]; exact .dnil
  | dcons he hr => intro _ hJ; rw [hJ.dfs]; exact .dcons he hr
  | dproj _ hc hk hl hts hH hT ih =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .dproj (ih (.st h₂)) hc hk hl hts hH (hT.trans h₁)
  | glob hg hts hH hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; exact .glob hg hts hH (hT.trans h₁)
  | perform hop hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; exact .perform hop (hT.trans h₁)
  | hanil =>
    intro J' hJ; cases J' <;> simp only [JLe] at hJ
    obtain ⟨rfl, rfl, rfl, rfl⟩ := hJ; exact .hanil
  | hatail hop hb hr =>
    intro J' hJ; cases J' <;> simp only [JLe] at hJ
    obtain ⟨rfl, rfl, rfl, rfl⟩ := hJ; exact .hatail hop hb hr
  | haabort hop hb hr =>
    intro J' hJ; cases J' <;> simp only [JLe] at hJ
    obtain ⟨rfl, rfl, rfl, rfl⟩ := hJ; exact .haabort hop hb hr
  | hactl hop hb hr =>
    intro J' hJ; cases J' <;> simp only [JLe] at hJ
    obtain ⟨rfl, rfl, rfl, rfl⟩ := hJ; exact .hactl hop hb hr
  | hdl hal hn hrf hψ he hφ₀ hOb hc hφ hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .hdl hal hn hrf hψ he hφ₀ hOb hc (Stage.le_trans hφ h₂) (hT.trans h₁)
  | hwith hℓ hal hrf hψ he hφ₀ hc hφ hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .hwith hℓ hal hrf hψ he hφ₀ hc (Stage.le_trans hφ h₂) (hT.trans h₁)
  | hctl hℓ hal hrf hψ hop hc htl hE hk hp hφ₀ ha hl hcm hφ hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .hctl hℓ hal hrf hψ hop hc htl hE hk hp hφ₀ ha hl hcm (Stage.le_trans hφ h₂) (hT.trans h₁)
  | hkeep hℓ hal he hφ₀ hc hnt hle hφ hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .hkeep hℓ hal he hφ₀ hc hnt hle (Stage.le_trans hφ h₂) (hT.trans h₁)
  | hkarm hℓ hal hop hE hk hp hφ₀ hc hnt ha hle hφ hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .hkarm hℓ hal hop hE hk hp hφ₀ hc hnt ha hle (Stage.le_trans hφ h₂) (hT.trans h₁)
  | hres hℓ hb hA ha hp hl hφ hT =>
    intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex
    exact .hres hℓ hb hA ha hp hl (Stage.le_trans hφ h₂) (hT.trans h₁)
  | hdone _ ih => intro _ hJ; obtain ⟨_, _, rfl, h₁, h₂⟩ := hJ.ex; exact .hdone (ih ⟨h₁, h₂⟩)
  | unw hℓ ha hφ => intro _ hJ; obtain ⟨_, _, rfl, _, h₂⟩ := hJ.ex; exact .unw hℓ ha (Stage.le_trans hφ h₂)
  | kont hl hC hT => intro _ hJ; obtain ⟨_, _, rfl, h₁, _⟩ := hJ.ex; exact .kont hl hC (hT.trans h₁)

end Flick
