import Formal.TySubst
import Formal.Held
import Formal.Cut

/-!
# Substitution lemma (Л2)

A variable with scheme `∀ᾱ. Θ' ⇒ τ` stands for a pure term typed at `τ` with `ᾱ`
fresh: `Γ`, `Θ` and `Ψ` shifted past them and `Θ'` assumed. The type
substitution lemma then gives the term every instance of the scheme whose facts
hold, and (Sub) any stage. The substituted terms are values or variables: they
have no `inside` form and hold no more than their schemes show (`Typ.holds`),
which keeps the premises of (Lam) and (Task).
-/

namespace Flick

variable {S : Sig}

/-- The facts of the context at the level of a scheme. -/
def Facts.at (Θ : Facts) (s : Scheme) : Facts := Facts.union (Θ.substT (shiftV s.k)) s.cs

/-- A substitution `σ : Γ → Γ'`: each variable goes to a pure term of its scheme,
without `inside`, holding no more than its scheme shows. -/
def SubOk (S : Sig) (Ψ : List Ty) (Θ : Facts) (Γ Γ' : List Scheme) (σ : Nat → Tm) : Prop :=
  ∀ (i : Nat) s, Γ[i]? = some s →
    Typ S (Ψ.map (Ty.up s.k)) (Θ.at s) (Γ'.map (Scheme.up s.k)) (σ i) (.ex s.t .pure) ∧
      (σ i).NoIns ∧ Caps.Sub (held Ψ Γ' (σ i)) s.reach

@[simp] theorem idTs_length (k : Nat) : (idTs k).length = k := by simp [idTs]

theorem atom_up_inst (k : Nat) (a : Atom) :
    (liftT k (shiftV k) a).substT (instT (idTs k)) = Ty.ident a := by
  cases a with
  | v i =>
    by_cases hi : i < k
    · simp [liftT, hi, Ty.substT, instT, idTs, Ty.ident]
    · have : ¬ i - k + k + k < k := by omega
      simp [liftT, hi, Ty.substT, instT, idTs, Ty.ident, Ty.up, shiftV, this]
      omega
  | r p => simp [liftT, Ty.up, Ty.substT, shiftV, instT, Ty.ident]

theorem Scheme.up_inst_self (s : Scheme) : (s.up s.k).inst (idTs s.k) = s.t := by
  simp only [Scheme.up, Scheme.substT, Scheme.inst, Ty.substT_comp]
  conv => rhs; rw [← s.t.substT_ident]
  exact Ty.substT_congr (atom_up_inst s.k)

theorem Scheme.up_holds_self (s : Scheme) (Θ : Facts) :
    (s.up s.k).Holds (Θ.at s) (idTs s.k) := by
  rintro C r ⟨C₀, r₀, h₀, rfl, rfl⟩
  have e₁ : (C₀.substT (liftT s.k (shiftV s.k))).substT (instT (idTs s.k)) = C₀ := by
    rw [Caps.substT_comp]; conv => rhs; rw [← C₀.substT_ident]
    congr 1; funext a; exact atom_up_inst _ a
  have e₂ : (r₀.substT (liftT s.k (shiftV s.k))).substT (instT (idTs s.k)) = r₀ := by
    rw [Ty.substT_comp]; conv => rhs; rw [← r₀.substT_ident]
    exact Ty.substT_congr (atom_up_inst _)
  rw [e₁, e₂]
  exact fun x hx => .inr ⟨C₀, .inr h₀, hx⟩

/-- A variable at the level of its own scheme. -/
theorem Typ.var_self {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {i : Nat} {s : Scheme}
    {φ : Stage} (hi : Γ[i]? = some s) :
    Typ S Ψ (Θ.at s) (Γ.map (Scheme.up s.k)) (.var i) (.ex s.t φ) :=
  .var (σ := s.up s.k) (ts := idTs s.k) (by simp [hi]) (by simp [Scheme.up, Scheme.substT])
    (s.up_holds_self Θ) (by rw [Scheme.up_inst_self]; exact .refl)

theorem Facts.up_substT (Θ : Facts) (k : Nat) (ts : List Ty) (h : ts.length = k) :
    (Θ.substT (shiftV k)).substT (instT ts) = Θ := by
  rw [Facts.substT_comp]; conv => rhs; rw [← Θ.substT_ident]
  congr 1; funext a
  cases a with
  | v i =>
    have : ¬ i + k < ts.length := by omega
    simp only [shiftV, Ty.substT, instT, dif_neg this, Ty.ident]
    congr 1; omega
  | r p => simp [shiftV, Ty.substT, instT, Ty.ident]

/-- A term at the level of a scheme, at an instance whose facts hold. -/
theorem Typ.instantiate {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {e : Tm} {s : Scheme}
    {ts : List Ty} (hts : ts.length = s.k) (hH : s.Holds Θ ts) (hn : e.NoIns)
    (h : Typ S (Ψ.map (Ty.up s.k)) (Θ.at s) (Γ.map (Scheme.up s.k)) e (.ex s.t .pure)) :
    Typ S Ψ Θ Γ e (.ex (s.inst ts) .pure) := by
  have := h.substT (instT ts) (Fits.noIns hn fun _ => rfl)
  simp only [List.map_map, Function.comp_def, Ty.up_substT _ _ _ hts,
    Scheme.up_substT _ _ _ hts, List.map_id', Flick.J.substT, Facts.at, Facts.union_substT,
    Facts.up_substT _ _ _ hts] at this
  refine this.cut fun C r hC x hx => ?_
  rcases hC x hx with h' | ⟨C', h₁ | ⟨C₀, r₀, h₀, rfl, hr⟩, h₂⟩
  · exact .inl h'
  · exact .inr ⟨C', h₁, h₂⟩
  · subst hr; exact hH C₀ r₀ h₀ x h₂

theorem held_up {Ψ Γ e} {s : Scheme} (k : Nat) (h : Caps.Sub (held Ψ Γ e) s.reach) :
    Caps.Sub (held (Ψ.map (Ty.up k)) (Γ.map (Scheme.up k)) e) (s.up k).reach := by
  have sup : ∀ (s' : Scheme) a, (s'.up k).reach a → ∃ b, s'.reach b ∧ a = b.up k := by
    intro s' a ha
    obtain ⟨b, hb, h'⟩ := (Scheme.reach_substT _ _ _).mp ha
    exact ⟨b, hb, by cases b <;> simp_all [shiftV, Atom.up]⟩
  rintro a (⟨i, s', hi, hs, ha⟩ | ⟨p, hp, rfl⟩ | ⟨l, t, hl, ht, ha⟩)
  · simp only [List.getElem?_map, Option.map_eq_some_iff] at hs
    obtain ⟨s₀, hs₀, rfl⟩ := hs
    obtain ⟨b, hb, rfl⟩ := sup _ _ ha
    exact (Scheme.reach_up _ _ _).mpr (h b (.inl ⟨i, s₀, hi, hs₀, hb⟩))
  · exact (Scheme.reach_up _ k (.r p)).mpr (h _ (.inr (.inl ⟨p, hp, rfl⟩)))
  · simp only [List.getElem?_map, Option.map_eq_some_iff] at ht
    obtain ⟨t₀, ht₀, rfl⟩ := ht
    obtain ⟨b, hb, rfl⟩ := (Ty.reach_up _ _ _).mp ha
    exact (Scheme.reach_up _ _ _).mpr (h b (.inr (.inr ⟨l, t₀, hl, ht₀, hb⟩)))

theorem held_down {Ψ Γ e} {s : Scheme}
    (h : Caps.Sub (held (Ψ.map (Ty.up s.k)) (Γ.map (Scheme.up s.k)) e) s.t.reach) :
    Caps.Sub (held Ψ Γ e) s.reach := by
  rintro a (⟨i, s', hi, hs, ha⟩ | ⟨p, hp, rfl⟩ | ⟨l, t, hl, ht, ha⟩)
  · have : (s'.up s.k).reach (a.up s.k) := (Scheme.reach_up _ _ _).mpr ha
    exact ⟨_, h _ (.inl ⟨i, _, hi, by simp [hs], this⟩), Atom.down_up _ _⟩
  · exact ⟨.r p, h _ (.inr (.inl ⟨p, hp, rfl⟩)), rfl⟩
  · refine ⟨a.up s.k, h _ (.inr (.inr ⟨l, t.up s.k, hl, by simp [ht], ?_⟩)), Atom.down_up _ _⟩
    exact (Ty.reach_up _ _ _).mpr ⟨a, ha, rfl⟩

/-- A value at the level of its scheme is a fit replacement. -/
theorem SubOk.value {Ψ : List Ty} {Θ : Facts} {Γ' : List Scheme} {v : Tm} {s : Scheme} (hv : Value v)
    (h : Typ S (Ψ.map (Ty.up s.k)) (Θ.at s) (Γ'.map (Scheme.up s.k)) v (.ex s.t .pure)) :
    v.NoIns ∧ Caps.Sub (held Ψ Γ' v) s.reach :=
  ⟨h.noIns hv, held_down (h.holds hv rfl).1⟩

theorem SubOk.lift {Ψ Θ Γ Γ' σ} (h : SubOk S Ψ Θ Γ Γ' σ) (Δ : List Scheme) {n} (hn : Δ.length = n) :
    SubOk S Ψ Θ (Δ ++ Γ) (Δ ++ Γ') (liftS n σ) := by
  intro i s hi
  subst hn
  unfold liftS
  split
  · rw [List.getElem?_append_left (by omega)] at hi
    refine ⟨Typ.var_self (by rw [List.getElem?_append_left (by omega)]; exact hi), ?_, ?_⟩
    · intro h; cases h
    rintro a (⟨j, s', hj, hs, ha⟩ | ⟨p, hp, _⟩ | ⟨l, _, hl, _⟩)
    · cases hj; rw [List.getElem?_append_left (by omega), hi] at hs; cases hs; exact ha
    · cases hp
    · cases hl
  · rw [List.getElem?_append_right (by omega)] at hi
    obtain ⟨ht, hnI, hc⟩ := h _ _ hi
    have hr : RenOk Γ' (Δ ++ Γ') (· + Δ.length) := fun j s hj => by
      show (Δ ++ Γ')[j + Δ.length]? = some s
      rw [List.getElem?_append_right (by omega)]; simpa using hj
    refine ⟨ht.rename fun j s hj => ?_, hnI.rename _, (held_rename hr fun j hj => ?_).trans hc⟩
    · rw [List.map_append, List.getElem?_append_right (by simp only [List.length_map]; omega)]
      simpa using hj
    · have := ht.frees j hj; simpa using this

theorem SubOk.up {Ψ Θ Γ Γ' σ} (h : SubOk S Ψ Θ Γ Γ' σ) (k : Nat) (Φ : Facts) :
    SubOk S (Ψ.map (Ty.up k)) (Facts.union (Θ.substT (shiftV k)) Φ) (Γ.map (Scheme.up k))
      (Γ'.map (Scheme.up k)) σ := by
  intro i s hi
  simp only [List.getElem?_map, Option.map_eq_some_iff] at hi
  obtain ⟨s₀, hs₀, rfl⟩ := hi
  obtain ⟨ht, hnI, hc⟩ := h _ _ hs₀
  have := ht.substT (liftT s₀.k (shiftV k)) (Fits.noIns hnI fun _ => by simp [liftT, Ty.up, Ty.substT, shiftV])
  simp only [Flick.J.substT] at this
  have e₁ : (Ψ.map (Ty.up s₀.k)).map (Ty.substT (liftT s₀.k (shiftV k))) =
      (Ψ.map (Ty.up k)).map (Ty.up s₀.k) := by
    simp [Ty.up_comm]; intro _ _; rfl
  have e₂ : (Γ'.map (Scheme.up s₀.k)).map (Scheme.substT (liftT s₀.k (shiftV k))) =
      (Γ'.map (Scheme.up k)).map (Scheme.up s₀.k) := by
    simp [Scheme.up_comm]; intro _ _; rfl
  have e₃ : (Θ.at s₀).substT (liftT s₀.k (shiftV k)) =
      Facts.union ((Θ.substT (shiftV k)).substT (shiftV s₀.k)) (s₀.up k).cs := by
    simp only [Facts.at, Facts.union_substT, Facts.up_comm]; rfl
  rw [e₁, e₂, e₃] at this
  refine ⟨this.cut ?_, hnI, held_up k hc⟩
  have hk : (s₀.up k).k = s₀.k := rfl
  simp only [Facts.at, hk]
  intro C r hC x hx
  rcases hC x hx with h' | ⟨C', h₁ | h₁, h₂⟩
  · exact .inl h'
  · obtain ⟨C₀, r₀, h₀, rfl, rfl⟩ := h₁
    exact .inr ⟨_, .inl ⟨C₀, r₀, .inl h₀, rfl, rfl⟩, h₂⟩
  · exact .inr ⟨C', .inr h₁, h₂⟩

theorem held_subst {Ψ Γ Γ' e} {σ : Nat → Tm} (hΓ : ∀ i, e.Free i → i < Γ.length)
    (hσ : ∀ i s, Γ[i]? = some s → Caps.Sub (held Ψ Γ' (σ i)) s.reach) :
    Caps.Sub (held Ψ Γ' (e.subst σ)) (held Ψ Γ e) := by
  have get : ∀ i, e.Free i → ∃ s, Γ[i]? = some s := fun i hi =>
    ⟨_, List.getElem?_eq_getElem (hΓ i hi)⟩
  rintro a (⟨j, s, hj, hs, ha⟩ | ⟨p, hp, rfl⟩ | ⟨l, t, hl, ht, ha⟩)
  · obtain ⟨i, hi, hj'⟩ := Tm.free_subst e σ j hj
    obtain ⟨s', hs'⟩ := get i hi
    exact .inl ⟨i, s', hi, hs', hσ i s' hs' a (.inl ⟨j, s, hj', hs, ha⟩)⟩
  · rcases Tm.has_subst e σ _ hp with h | ⟨i, hi, h⟩
    · exact .inr (.inl ⟨p, h, rfl⟩)
    · obtain ⟨s', hs'⟩ := get i hi
      exact .inl ⟨i, s', hi, hs', hσ i s' hs' _ (.inr (.inl ⟨p, h, rfl⟩))⟩
  · rcases Tm.has_subst e σ _ hl with h | ⟨i, hi, h⟩
    · exact .inr (.inr ⟨l, t, h, ht, ha⟩)
    · obtain ⟨s', hs'⟩ := get i hi
      exact .inl ⟨i, s', hi, hs', hσ i s' hs' _ (.inr (.inr ⟨l, t, h, ht, ha⟩))⟩

theorem noIns_subst {Γ : List Scheme} {e : Tm} {σ : Nat → Tm} (hΓ : ∀ i, e.Free i → i < Γ.length) (hn : e.NoIns)
    (hσ : ∀ i s, Γ[i]? = some s → (σ i).NoIns) : (e.subst σ).NoIns := by
  intro h
  rcases Tm.has_subst e σ _ h with h | ⟨i, hi, h⟩
  · exact hn h
  · exact hσ i _ (List.getElem?_eq_getElem (hΓ i hi)) h

theorem SubOk.mono {Ψ Θ Θ' Γ Γ'} {σ : Nat → Tm} (h : SubOk S Ψ Θ Γ Γ' σ) (hΘ : Entails Θ Θ') :
    SubOk S Ψ Θ' Γ Γ' σ := fun i s hi =>
  let ⟨ht, hn, hc⟩ := h i s hi
  ⟨ht.cut (hΘ.lift s.k s.cs), hn, hc⟩

theorem SubOk.held {Ψ : List Ty} {Θ : Facts} {Γ Γ' : List Scheme} {e : Tm} {T : Caps} {σ : Nat → Tm} (hσ : SubOk S Ψ Θ Γ Γ' σ)
    (hΓ : ∀ i, e.Free i → i < Γ.length) (h : Caps.Sub (held Ψ Γ e) T) :
    Caps.Sub (held Ψ Γ' (e.subst σ)) T :=
  (held_subst hΓ fun i s hs => (hσ i s hs).2.2).trans h

theorem SubOk.noIns {Ψ : List Ty} {Θ : Facts} {Γ Γ' : List Scheme} {e : Tm} {σ : Nat → Tm} (hσ : SubOk S Ψ Θ Γ Γ' σ)
    (hΓ : ∀ i, e.Free i → i < Γ.length) (h : e.NoIns) : (e.subst σ).NoIns :=
  noIns_subst hΓ h fun i s hs => (hσ i s hs).2.1

theorem Typ.subst {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ {Γ' σ}, SubOk S Ψ Θ Γ Γ' σ → Typ S Ψ Θ Γ' (e.subst σ) J := by
  induction h with
  | var hi hts hH hT =>
    intro _ _ hσ
    obtain ⟨ht, hnI, _⟩ := hσ _ _ hi
    exact (Typ.instantiate hts hH hnI ht).sub ⟨hT, Stage.pure_le _⟩
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
  | ctor hK hk hT => intro _ _ _; exact .ctor hK hk hT
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
  | provideL _ _ h₁ h₀ h₂ ih₁ ih₂ => intro _ _ hσ; exact .provideL (ih₁ hσ) (ih₂ hσ) h₁ h₀ h₂
  | provideE _ _ h₁ h₀ h₂ ih₁ ih₂ => intro _ _ hσ; exact .provideE (ih₁ hσ) (ih₂ hσ) h₁ h₀ h₂
  | with_ hv _ _ h₁ h₂ ih₁ ih₂ =>
    intro _ _ hσ; exact .with_ hv.subst (ih₁ hσ) (ih₂ hσ) h₁ h₂
  | enil hρ => intro _ _ _; exact .enil hρ
  | econs hv hθ _ _ hρ ih₁ ih₂ =>
    intro _ σ hσ; exact .econs hv.subst ((Homo.subst σ).isEnv hθ) (ih₁ hσ) (ih₂ hσ) hρ
  | bind _ hρ ih => intro _ _ hσ; exact .bind (ih hσ) hρ
  | join _ _ hρ ih₁ ih₂ => intro _ _ hσ; exact .join (ih₁ hσ) (ih₂ hσ) hρ
  | scope hne hF he hφ' hφ hT ih =>
    intro _ _ hσ
    have hΓ : ∀ i, (Tm.scope _).Free i → i < _ := fun i h => by
      have := he.frees _ h; simp at this; omega
    exact .scope (SubOk.noIns (e := .scope _) hσ hΓ hne) hF (ih (SubOk.lift (SubOk.up hσ 1 _) [_] rfl)) hφ' hφ hT
  | inside hF _ hφ' ht hε hφ hT ih =>
    intro _ _ hσ; exact .inside hF (ih (hσ.mono (Entails.union_left _ _))) hφ' ht hε hφ hT
  | cap => intro _ _ _; exact .cap
  | own => intro _ _ _; exact .own

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
    refine ⟨Typ.var_self hi', ?_, ?_⟩
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
