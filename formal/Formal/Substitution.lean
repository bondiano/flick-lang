import Formal.TySubst

/-!
# Substitution lemma (Л2)

A variable with scheme `∀ᾱ.τ` stands for a pure term typed at `τ` with `ᾱ`
fresh: `Γ` and `Ψ` shifted past them. The type substitution lemma then gives the
term every instance of the scheme, and (Sub) any stage.
-/

namespace Flick

variable {S : Sig}

/-- A substitution `σ : Γ → Γ'`: each variable goes to a pure term of its scheme. -/
def SubOk (S : Sig) (Ψ : List Ty) (Γ Γ' : List Scheme) (σ : Nat → Tm) : Prop :=
  ∀ (i : Nat) s, Γ[i]? = some s →
    Typ S (Ψ.map (Ty.up s.k)) (Γ'.map (Scheme.up s.k)) (σ i) (.ex s.t .pure)

/-- `α₀ … αₖ₋₁`. -/
def idTs (k : Nat) : List Ty := (List.range k).map Ty.tvar

@[simp] theorem idTs_length (k : Nat) : (idTs k).length = k := by simp [idTs]

theorem Scheme.up_inst_self (s : Scheme) : (s.up s.k).inst (idTs s.k) = s.t := by
  simp only [Scheme.up, Scheme.substT, Scheme.inst, Ty.substT_comp]
  conv => rhs; rw [← s.t.substT_tvar]
  apply Ty.substT_congr
  intro i
  unfold liftT
  by_cases hi : i < s.k
  · simp [hi, Ty.substT, instT, idTs]
  · have : ¬ i - s.k + s.k + s.k < s.k := by omega
    simp [hi, Ty.substT, instT, idTs, Ty.up, shiftV, this]
    omega

/-- A variable at the level of its own scheme. -/
theorem Typ.var_self {Ψ : List Ty} {Γ : List Scheme} {i : Nat} {s : Scheme} {φ : Stage} (hi : Γ[i]? = some s) :
    Typ S Ψ (Γ.map (Scheme.up s.k)) (.var i) (.ex s.t φ) :=
  .var (σ := s.up s.k) (ts := idTs s.k) (by simp [hi]) (by simp [Scheme.up, Scheme.substT])
    (by rw [Scheme.up_inst_self]; exact .refl)

/-- A term at the level of a scheme, instantiated. -/
theorem Typ.instantiate {Ψ : List Ty} {Γ : List Scheme} {e : Tm} {s : Scheme} {φ : Stage} {ts : List Ty} (hts : ts.length = s.k)
    (h : Typ S (Ψ.map (Ty.up s.k)) (Γ.map (Scheme.up s.k)) e (.ex s.t φ)) :
    Typ S Ψ Γ e (.ex (s.inst ts) φ) := by
  have := h.substT (instT ts)
  simp only [List.map_map, Function.comp_def, Ty.up_substT _ _ _ hts,
    Scheme.up_substT _ _ _ hts, List.map_id', Flick.J.substT] at this
  exact this

theorem SubOk.lift {Ψ Γ Γ' σ} (h : SubOk S Ψ Γ Γ' σ) (Δ : List Scheme) {n} (hn : Δ.length = n) :
    SubOk S Ψ (Δ ++ Γ) (Δ ++ Γ') (liftS n σ) := by
  intro i s hi
  subst hn
  unfold liftS
  split
  · rw [List.getElem?_append_left (by omega)] at hi
    exact Typ.var_self (by rw [List.getElem?_append_left (by omega)]; exact hi)
  · rw [List.getElem?_append_right (by omega)] at hi
    exact (h _ _ hi).rename fun j s hj => by
      rw [List.map_append, List.getElem?_append_right (by simp only [List.length_map]; omega)]
      simpa using hj

theorem SubOk.up {Ψ Γ Γ' σ} (h : SubOk S Ψ Γ Γ' σ) (k : Nat) :
    SubOk S (Ψ.map (Ty.up k)) (Γ.map (Scheme.up k)) (Γ'.map (Scheme.up k)) σ := by
  intro i s hi
  simp only [List.getElem?_map, Option.map_eq_some_iff] at hi
  obtain ⟨s₀, hs₀, rfl⟩ := hi
  have := (h _ _ hs₀).substT (liftT s₀.k (shiftV k))
  simp only [Flick.J.substT] at this
  have e₁ : (Ψ.map (Ty.up s₀.k)).map (Ty.substT (liftT s₀.k (shiftV k))) =
      (Ψ.map (Ty.up k)).map (Ty.up s₀.k) := by
    simp [Ty.up_comm]; intro _ _; rfl
  have e₂ : (Γ'.map (Scheme.up s₀.k)).map (Scheme.substT (liftT s₀.k (shiftV k))) =
      (Γ'.map (Scheme.up k)).map (Scheme.up s₀.k) := by
    simp [Scheme.up_comm]; intro _ _; rfl
  rw [e₁, e₂] at this
  exact this

theorem Typ.subst {Ψ Γ e J} (h : Typ S Ψ Γ e J) :
    ∀ {Γ' σ}, SubOk S Ψ Γ Γ' σ → Typ S Ψ Γ' (e.subst σ) J := by
  induction h with
  | var hi hts hT =>
    intro _ _ hσ; exact (Typ.instantiate hts (hσ _ _ hi)).sub ⟨hT, Stage.pure_le _⟩
  | lam _ hT ih => intro _ _ hσ; exact .lam (ih (hσ.lift [_] rfl)) hT
  | app _ _ hψ hT ih₁ ih₂ => intro _ _ hσ; exact .app (ih₁ hσ) (ih₂ hσ) hψ hT
  | let_ _ _ ih₁ ih₂ => intro _ _ hσ; exact .let_ (ih₁ hσ) (ih₂ (hσ.lift [_] rfl))
  | letGen _ _ ih₁ ih₂ =>
    intro _ _ hσ; exact .letGen (ih₁ (hσ.up _)) (ih₂ (hσ.lift [_] rfl))
  | letrec _ _ ih₁ ih₂ =>
    intro _ _ hσ; exact .letrec (ih₁ (hσ.lift [_, _] rfl)) (ih₂ (hσ.lift [_] rfl))
  | fix _ hT ih => intro _ _ hσ; exact .fix (ih (hσ.lift [_, _] rfl)) hT
  | lit => intro _ _ _; exact .lit
  | loc hl => intro _ _ _; exact .loc hl
  | const hc hT => intro _ _ _; exact .const hc hT
  | ctor hK hT => intro _ _ _; exact .ctor hK hT
  | rnil => intro _ _ _; exact .rnil
  | rcons hR hl _ _ ih₁ ih₂ => intro _ _ hσ; exact .rcons hR hl (ih₁ hσ) (ih₂ hσ)
  | proj _ hf hT ih => intro _ _ hσ; exact .proj (ih hσ) hf hT
  | update _ hf _ ih₁ ih₂ => intro _ _ hσ; exact .update (ih₁ hσ) hf (ih₂ hσ)
  | match_ _ h₂ hc ih₁ ih₂ =>
    intro _ σ hσ
    exact .match_ (ih₁ hσ) (ih₂ hσ)
      (by rw [Typ.unguarded_subst h₂ σ rfl]; exact hc.map (Homo.subst σ))
  | anil => intro _ _ _; exact .anil
  | acons hp _ _ ih₁ ih₂ => intro _ _ hσ; exact .acons hp (ih₁ (hσ.lift _ (by simpa using hp.binds))) (ih₂ hσ)
  | aguard hp _ _ _ ih₁ ih₂ ih₃ =>
    intro _ _ hσ
    exact .aguard hp (ih₁ (hσ.lift _ (by simpa using hp.binds))) (ih₂ (hσ.lift _ (by simpa using hp.binds))) (ih₃ hσ)
  | todo => intro _ _ _; exact .todo
  | task _ hψ hT ih => intro _ _ hσ; exact .task (ih hσ) hψ hT
  | await _ hφ hT ih => intro _ _ hσ; exact .await (ih hσ) hφ hT
  | raise _ hφ ih => intro _ _ hσ; exact .raise (ih hσ) hφ
  | raiseAdt _ hφ ih => intro _ _ hσ; exact .raiseAdt (ih hσ) hφ
  | errVal hv _ hE ih => intro _ _ hσ; exact .errVal hv.subst (ih hσ) hE
  | tryR _ _ hφ hs ih₁ ih₂ => intro _ _ hσ; exact .tryR (ih₁ hσ) (ih₂ hσ) hφ hs
  | tryP _ _ ih₁ ih₂ => intro _ _ hσ; exact .tryP (ih₁ hσ) (ih₂ hσ)
  | handle hv _ _ hc hφ ih₁ ih₂ =>
    intro _ σ hσ
    exact .handle hv.subst (ih₁ hσ) (ih₂ hσ) (hc.map (Homo.subst σ) hv) hφ
  | hnil hH => intro _ _ _; exact .hnil hH
  | hcons hp _ _ hH ih₁ ih₂ =>
    intro _ _ hσ; exact .hcons hp (ih₁ (hσ.lift _ (by simpa using hp.binds))) (ih₂ hσ) hH
  | hguard hp _ _ _ hH ih₁ ih₂ ih₃ =>
    intro _ _ hσ
    exact .hguard hp (ih₁ (hσ.lift _ (by simpa using hp.binds))) (ih₂ (hσ.lift _ (by simpa using hp.binds))) (ih₃ hσ) hH
  | provideL _ _ h₁ h₂ ih₁ ih₂ => intro _ _ hσ; exact .provideL (ih₁ hσ) (ih₂ hσ) h₁ h₂
  | provideE _ _ h₁ h₂ ih₁ ih₂ => intro _ _ hσ; exact .provideE (ih₁ hσ) (ih₂ hσ) h₁ h₂
  | with_ hv _ _ h₁ h₂ ih₁ ih₂ =>
    intro _ _ hσ; exact .with_ hv.subst (ih₁ hσ) (ih₂ hσ) h₁ h₂
  | enil hρ => intro _ _ _; exact .enil hρ
  | econs hv hθ _ _ hρ ih₁ ih₂ =>
    intro _ σ hσ; exact .econs hv.subst ((Homo.subst σ).isEnv hθ) (ih₁ hσ) (ih₂ hσ) hρ
  | bind _ hρ ih => intro _ _ hσ; exact .bind (ih hσ) hρ
  | join _ _ hρ ih₁ ih₂ => intro _ _ hσ; exact .join (ih₁ hσ) (ih₂ hσ) hρ

/-- `e[x := v]` for `x` with a scheme: `v` is typed with the scheme's variables fresh. -/
theorem SubOk.one {Ψ v} {s : Scheme} (h : Typ S (Ψ.map (Ty.up s.k)) [] v (.ex s.t .pure)) :
    SubOk S Ψ [s] [] (inst [v]) := by
  intro i s' hi
  cases i with
  | zero => simp at hi; subst hi; exact h
  | succ i => simp at hi

/-- `δ` gives the variables `Δ` their types in `Γ`, with `P`. -/
def Inst (S : Sig) (Ψ : List Ty) (Γ : List Scheme) (δ : List Tm) (Δ : List Ty) : Prop :=
  δ.length = Δ.length ∧
    ∀ (i : Nat) v t, δ[i]? = some v → Δ[i]? = some t → Typ S Ψ Γ v (.ex t .pure)

theorem Inst.nil {Ψ Γ} : Inst S Ψ Γ [] [] := ⟨rfl, by simp⟩

theorem Inst.cons {Ψ Γ v a δ Δ} (hv : Typ S Ψ Γ v (.ex a .pure)) (h : Inst S Ψ Γ δ Δ) :
    Inst S Ψ Γ (v :: δ) (a :: Δ) := by
  refine ⟨by simp [h.1], fun i w t hw ht => ?_⟩
  cases i with
  | zero => simp at hw ht; subst hw ht; exact hv
  | succ i => exact h.2 i w t (by simpa using hw) (by simpa using ht)

theorem Inst.append {Ψ Γ δ₁ δ₂ Δ₁ Δ₂} (h₁ : Inst S Ψ Γ δ₁ Δ₁) (h₂ : Inst S Ψ Γ δ₂ Δ₂) :
    Inst S Ψ Γ (δ₁ ++ δ₂) (Δ₁ ++ Δ₂) := by
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

theorem Inst.ok {Ψ Γ δ Δ} (h : Inst S Ψ Γ δ Δ) : SubOk S Ψ (Δ.map .mono ++ Γ) Γ (inst δ) := by
  intro i s hi
  have hl := h.1
  unfold inst
  split
  · next hlt =>
    rw [List.getElem?_append_left (by simp; omega)] at hi
    simp only [List.getElem?_map, Option.map_eq_some_iff] at hi
    obtain ⟨t, ht, rfl⟩ := hi
    simpa using h.2 i _ t (List.getElem?_eq_getElem hlt) ht
  · rw [List.getElem?_append_right (by simp; omega)] at hi
    exact Typ.var_self (by simpa [h.1] using hi)

/-- `e[δ]` for a body typed under `Δ`. -/
theorem Typ.inst {Ψ Γ e J δ Δ} (h : Typ S Ψ (Δ.map .mono ++ Γ) e J) (hδ : Inst S Ψ Γ δ Δ) :
    Typ S Ψ Γ (e.subst (inst δ)) J :=
  h.subst hδ.ok

end Flick
