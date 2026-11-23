import Formal.Equiv.SubstRename

/-!
# Contextual equivalence: substitution of related terms
-/

namespace Flick.Equiv

open Flick.Elab (All2)

theorem SubR.lift {σ σ' : Nat → Tm} (h : SubR W σ σ') (n : Nat) :
    SubR W (liftS n σ) (liftS n σ') := by
  intro i
  unfold liftS
  split
  · exact ⟨fun _ hy => .var hy, by simp [Tm.NoIns, Tm.Has], .inl ⟨i, rfl⟩, .inl ⟨i, rfl⟩⟩
  · refine ⟨fun y hy => ((h _).1 y hy).rename _, Tm.NoIns.rename _ (h _).2.1, ?_, ?_⟩
    · rcases (h _).2.2.1 with ⟨j, hj⟩ | hv
      · rw [hj]; exact .inl ⟨j + n, rfl⟩
      · exact .inr hv.rename
    · rcases (h _).2.2.2 with ⟨j, hj⟩ | hv
      · rw [hj]; exact .inl ⟨j + n, rfl⟩
      · exact .inr hv.rename

theorem SubR.noIns {σ σ' : Nat → Tm} (h : SubR W σ σ') : ∀ i, (σ' i).NoIns := fun i => (h i).2.1

theorem SubR.leftVarVal {σ σ' : Nat → Tm} (h : SubR W σ σ') :
    ∀ i, (∃ j, σ i = .var j) ∨ Value (σ i) := fun i => (h i).2.2.1

theorem SubR.rightVarVal {σ σ' : Nat → Tm} (h : SubR W σ σ') :
    ∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i) := fun i => (h i).2.2.2

theorem Sim.subst {x : Pos} {a b : Tm} (h : Sim W x a b) :
    ∀ σ σ', SubR W σ σ' → Sim W x (a.subst σ) (b.subst σ') := by
  induction h with
  | var hx => intro σ σ' hσ; exact (hσ _).1 _ hx
  | dropA hb _ ih =>
    intro σ σ' hσ; subst hb; simp only [Tm.subst]; exact .dropA (subst_up _ _) (ih _ _ hσ)
  | dropB hn hb _ ih =>
    intro σ σ' hσ; subst hb; simp only [Tm.subst]; exact .dropB hn (subst_up _ _) (ih _ _ hσ)
  | dropC hb hn _ ih =>
    intro σ σ' hσ; subst hb; simp only [Tm.subst]
    exact .dropC (subst_up _ _) (Tm.NoIns.subst hn hσ.noIns) (ih _ _ hσ)
  | forceC hn _ ih => intro σ σ' hσ; exact .forceC (Tm.NoIns.subst hn hσ.noIns) (ih _ _ hσ)
  | bornA hB hv hv' _ ih =>
    intro σ σ' hσ; simp only [Tm.subst, liftS]; exact .bornA hB hv.subst hv'.subst (ih _ _ hσ)
  | bornB hB hv hv' _ ih =>
    intro σ σ' hσ; simp only [Tm.subst, liftS]; exact .bornB hB hv.subst hv'.subst (ih _ _ hσ)
  | bornS hB hl hc hs =>
    intro σ σ' _; rw [hs.subst hc]; exact .bornS hB hl hc hs
  | bornV hB hl hc hc' hn =>
    intro σ σ' _; simp only [Tm.subst]; rw [hc.subst, hc'.subst]; exact .bornV hB hl hc hc' hn
  | dead hB hn => intro σ σ' hσ; exact .dead hB (Tm.NoIns.subst hn hσ.noIns)
  | hloc hB hH => intro _ _ _; exact .hloc hB hH
  | o4A hB hf hbr hpe hpe' _ _ hne hnb ihe ihb =>
    subst hf hbr; intro σ σ' hσ; simp only [Tm.subst]
    exact .o4A hB (by simp only [subst_up2]) (rot_subst _ _)
      (hpe.subst hσ.leftVarVal) (hpe'.subst hσ.rightVarVal) (ihe _ _ hσ)
      (ihb _ _ ((hσ.lift 2).lift 1)) (Tm.NoIns.subst hne hσ.noIns)
      (Tm.NoIns.subst hnb ((hσ.lift 2).lift 1).noIns)
  | o4B hB hbr hf' hpe hpe' _ _ hne hnb ihe ihb =>
    subst hbr hf'; intro σ σ' hσ; simp only [Tm.subst]
    exact .o4B hB (rot_subst _ _) (by simp only [subst_up2])
      (hpe.subst hσ.leftVarVal) (hpe'.subst hσ.rightVarVal) (ihe _ _ hσ)
      (ihb _ _ ((hσ.lift 2).lift 1)) (Tm.NoIns.subst hne hσ.noIns)
      (Tm.NoIns.subst hnb ((hσ.lift 2).lift 1).noIns)
  | o4Fix1 hB hr hf hf' hcl hpe he _ hnb _ ihb =>
    subst hf hf'; intro σ σ' hσ; simp only [Tm.subst]
    exact .o4Fix1 hB hr (by simp only [subst_up2, hcl.subst]) (subst_loc_subst2 _ _ _)
      hcl hpe he (ihb _ _ ((hσ.lift 2).lift 1))
      (Tm.NoIns.subst hnb ((hσ.lift 2).lift 1).noIns)
  | o4Fix2 hB hl hf hf' hcl hpe he _ hne hnb _ ihb =>
    subst hf hf'; intro σ σ' hσ; simp only [Tm.subst]
    exact .o4Fix2 hB hl (subst_loc_subst2 _ _ _) (by simp only [subst_up2, hcl.subst])
      hcl hpe he (ihb _ _ ((hσ.lift 2).lift 1)) hne
      (Tm.NoIns.subst hnb ((hσ.lift 2).lift 1).noIns)
  | o4Alloc1 hB hr hs' hcl hpe he _ hnc _ ihc =>
    subst hs'; intro σ σ' hσ; simp only [Tm.subst, hcl.subst]
    exact .o4Alloc1 hB hr (subst_loc_subst _ _ _) hcl hpe he (ihc _ _ (hσ.lift 1))
      (Tm.NoIns.subst hnc (hσ.lift 1).noIns)
  | o4AllocR hB hl hs hcl hpe he _ _ ihc =>
    subst hs; intro σ σ' hσ; simp only [Tm.subst, hcl.subst]
    exact .o4AllocR hB hl (subst_loc_subst _ _ _) hcl hpe he (ihc _ _ (hσ.lift 1))
  | o4Bind hB _ ih =>
    intro σ σ' hσ; simp only [Tm.subst]
    refine .o4Bind hB ?_
    rw [← subst_loc_subst]; exact ih _ _ hσ
  | o4Settle hB hH hhl hce hct hpt =>
    intro σ σ' _; simp only [Tm.subst]; rw [hct.subst]
    exact .o4Settle hB hH hhl hce hct hpt
  | doneF hB hd hv' hc' hn =>
    intro σ σ' _; simp only [Tm.subst]; rw [hc'.subst]; exact .doneF hB hd hv' hc' hn
  | doneR hB hd hv hc =>
    intro σ σ' _; simp only [Tm.subst]; rw [hc.subst]; exact .doneR hB hd hv hc
  | againF1 hB hc _ hev hev' _ hσn hσv hσv' ihe ihσ =>
    intro τ τ' hτ; simp only [Tm.subst, subForce_subst, subVar_subst]
    exact .againF1 hB hc (fun y hy => ihe y hy τ τ' hτ)
      (varVal_subst hev hτ.leftVarVal) (varVal_subst hev' hτ.rightVarVal)
      (fun i y hy => ihσ i y hy τ τ' hτ) (fun i => Tm.NoIns.subst (hσn i) hτ.noIns)
      (fun i => varVal_subst (hσv i) hτ.leftVarVal) (fun i => varVal_subst (hσv' i) hτ.rightVarVal)
  | againF2 hx hB hl _ hc _ hσn hσv hσv' iht ihσ =>
    intro τ τ' hτ; simp only [Tm.subst, subForce_subst, subVar_subst]
    exact .againF2 hx hB hl (iht τ τ' hτ) hc (fun i y hy => ihσ i y hy τ τ' hτ)
      (fun i => Tm.NoIns.subst (hσn i) hτ.noIns)
      (fun i => varVal_subst (hσv i) hτ.leftVarVal) (fun i => varVal_subst (hσv' i) hτ.rightVarVal)
  | againF3 hB hd hv hv' hcv hcv' hnv' _ hc _ hσn hσv hσv' ihv ihσ =>
    intro τ τ' hτ; simp only [Tm.subst, hcv.subst, hcv'.subst, subForce_subst, subVar_subst]
    exact .againF3 hB hd hv hv' hcv hcv' hnv' (by simpa [hcv.subst, hcv'.subst] using ihv τ τ' hτ) hc
      (fun i y hy => ihσ i y hy τ τ' hτ) (fun i => Tm.NoIns.subst (hσn i) hτ.noIns)
      (fun i => varVal_subst (hσv i) hτ.leftVarVal) (fun i => varVal_subst (hσv' i) hτ.rightVarVal)
  | againR1 hB hc _ hev hev' _ hσn hσv hσv' ihe ihσ =>
    intro τ τ' hτ; simp only [Tm.subst, subForce_subst, subVar_subst]
    exact .againR1 hB hc (fun y hy => ihe y hy τ τ' hτ)
      (varVal_subst hev hτ.leftVarVal) (varVal_subst hev' hτ.rightVarVal)
      (fun i y hy => ihσ i y hy τ τ' hτ) (fun i => Tm.NoIns.subst (hσn i) hτ.noIns)
      (fun i => varVal_subst (hσv i) hτ.leftVarVal) (fun i => varVal_subst (hσv' i) hτ.rightVarVal)
  | againR2 hx hB hl _ hc _ hσn hσv hσv' iht ihσ =>
    intro τ τ' hτ; simp only [Tm.subst, subForce_subst, subVar_subst]
    exact .againR2 hx hB hl (iht τ τ' hτ) hc (fun i y hy => ihσ i y hy τ τ' hτ)
      (fun i => Tm.NoIns.subst (hσn i) hτ.noIns)
      (fun i => varVal_subst (hσv i) hτ.leftVarVal) (fun i => varVal_subst (hσv' i) hτ.rightVarVal)
  | againR3 hB hd hv hv' hcv hcv' hnv' _ hc _ hσn hσv hσv' ihv ihσ =>
    intro τ τ' hτ; simp only [Tm.subst, hcv.subst, hcv'.subst, subForce_subst, subVar_subst]
    exact .againR3 hB hd hv hv' hcv hcv' hnv' (by simpa [hcv.subst, hcv'.subst] using ihv τ τ' hτ) hc
      (fun i y hy => ihσ i y hy τ τ' hτ) (fun i => Tm.NoIns.subst (hσn i) hτ.noIns)
      (fun i => varVal_subst (hσv i) hτ.leftVarVal) (fun i => varVal_subst (hσv' i) hτ.rightVarVal)
  | letRaise hB hv hv' _ ih =>
    intro τ τ' hτ; simp only [Tm.subst]; exact .letRaise hB hv.subst hv'.subst (ih τ τ' hτ)
  | letUnw hB hl _ ih =>
    intro τ τ' hτ; simp only [Tm.subst]; exact .letUnw hB hl (ih τ τ' hτ)
  | letTodo hB =>
    intro τ τ' _; simp only [Tm.subst]; exact .letTodo hB
  | appK hs _ _ ih₁ ih₂ => intro σ σ' hσ; exact .appK (hs.subst σ) (ih₁ _ _ hσ) (ih₂ _ _ hσ)
  | appV hv _ _ ih₁ ih₂ => intro σ σ' hσ; exact .appV hv.subst (ih₁ _ _ hσ) (ih₂ _ _ hσ)
  | rconsK hs _ _ ih₁ ih₂ => intro σ σ' hσ; exact .rconsK (hs.subst σ) (ih₁ _ _ hσ) (ih₂ _ _ hσ)
  | rconsV hv _ _ ih₁ ih₂ => intro σ σ' hσ; exact .rconsV hv.subst (ih₁ _ _ hσ) (ih₂ _ _ hσ)
  | updK hs _ _ ih₁ ih₂ => intro σ σ' hσ; exact .updK (hs.subst σ) (ih₁ _ _ hσ) (ih₂ _ _ hσ)
  | updV hv _ _ ih₁ ih₂ => intro σ σ' hσ; exact .updV hv.subst (ih₁ _ _ hσ) (ih₂ _ _ hσ)
  | aconsD hd hn _ ih =>
    intro σ σ' hσ; simp only [Tm.subst]
    exact .aconsD (hd.imp (·.subst _) (·.subst _)) (Tm.NoIns.subst hn (hσ.lift _).noIns) (ih _ _ hσ)
  | _ =>
    intro σ σ' hσ; simp only [Tm.subst]
    constructor <;> first
      | assumption
      | (apply_assumption <;> first | exact hσ | exact hσ.lift _)
      | exact Tm.NoIns.subst ‹_› (hσ.lift _).noIns
      | exact Tm.NoIns.subst ‹_› hσ.noIns

/-- Substitutions used when firing O2a's `let`: related at `any`, right side has `NoIns`, and
every slot is either related at `top` or has `LzShape` on one side (so it cannot stand at `top`). -/
def SubD (W : World) (σ₁ σ₂ : Nat → Tm) : Prop :=
  ∀ i, Sim W .any (σ₁ i) (σ₂ i) ∧ (σ₂ i).NoIns ∧
    (Sim W .top (σ₁ i) (σ₂ i) ∨ (σ₁ i).LzShape ∨ (σ₂ i).LzShape)

theorem SubD.noIns {σ₁ σ₂ : Nat → Tm} (h : SubD W σ₁ σ₂) : ∀ i, (σ₂ i).NoIns := fun i => (h i).2.1

theorem SubD.lift {σ₁ σ₂ : Nat → Tm} (h : SubD W σ₁ σ₂) (n : Nat) :
    SubD W (liftS n σ₁) (liftS n σ₂) := by
  intro i
  unfold liftS
  split
  · exact ⟨.var (by decide), by simp [Tm.NoIns, Tm.Has], .inl (.var (by decide))⟩
  · exact ⟨(h _).1.rename _, Tm.NoIns.rename _ (h _).2.1,
      (h _).2.2.imp (·.rename _) (fun h' => h'.imp (·.rename _) (·.rename _))⟩

/-- Substituting a `SubD` pair into a source term `c₀` yields related terms whenever neither side
puts a lazy/force form at a `top` position. -/
theorem _root_.Flick.Tm.Src.subst_any :
    ∀ {c₀ : Tm}, c₀.Src → ∀ {σ₁ σ₂ : Nat → Tm}, SubD W σ₁ σ₂ →
      ∀ {x : Pos}, x ≠ .park → ¬ (c₀.subst σ₁).LzTop x → ¬ (c₀.subst σ₂).LzTop x →
        Sim W x (c₀.subst σ₁) (c₀.subst σ₂) := by
  have any : Pos.any ≠ .park := by decide
  have top : Pos.top ≠ .park := by decide
  have fr_any : ∀ {x : Pos}, x ≠ .park → x.fr = .any := fun {x} hx => by cases x <;> simp_all
  intro c₀
  induction c₀ with
  | var i =>
    intro _ σ₁ σ₂ hσ x hx h₁ h₂
    cases x with
    | any => exact (hσ i).1
    | top =>
      rcases (hσ i).2.2 with ht | hs₁ | hs₂
      · exact ht
      · exact (h₁ (.here hs₁)).elim
      · exact (h₂ (.here hs₂)).elim
    | park => exact (hx rfl).elim
  | lit => intro _ _ _ _ _ _ _ _; exact .lit
  | const => intro _ _ _ _ _ _ _ _; exact .const
  | ctor => intro _ _ _ _ _ _ _ _; exact .ctor
  | rnil => intro _ _ _ _ _ _ _ _; exact .rnil
  | anil => intro _ _ _ _ _ _ _ _; exact .anil
  | todo => intro _ _ _ _ _ _ _ _; exact .todo
  | glob => intro _ _ _ _ _ _ _ _; exact .glob
  | lam b ih =>
    intro hc σ₁ σ₂ hσ x _ h₁ h₂
    exact .lam (ih hc (hσ.lift 1) any (h₁ ∘ .lam) (h₂ ∘ .lam))
      (Tm.NoIns.subst (Tm.Src.noIns (e := b) hc) (hσ.lift 1).noIns)
  | fix b ih =>
    intro hc σ₁ σ₂ hσ x _ h₁ h₂
    exact .fix (ih hc (hσ.lift 2) any (h₁ ∘ .fix) (h₂ ∘ .fix))
      (Tm.NoIns.subst (Tm.Src.noIns (e := b) hc) (hσ.lift 2).noIns)
  | task b ih =>
    intro hc σ₁ σ₂ hσ x _ h₁ h₂
    exact .task (ih hc hσ any (h₁ ∘ .task) (h₂ ∘ .task))
      (Tm.NoIns.subst (Tm.Src.noIns (e := b) hc) hσ.noIns)
  | await b ih =>
    intro hc σ₁ σ₂ hσ x hx h₁ h₂
    exact .await ((fr_any hx).symm ▸ ih hc hσ any (h₁ ∘ .await) (h₂ ∘ .await))
  | lazy b ih =>
    intro hc σ₁ σ₂ hσ x _ h₁ h₂
    exact .lazy (ih hc hσ any (h₁ ∘ .lazy) (h₂ ∘ .lazy))
  | force b ih =>
    intro hc σ₁ σ₂ hσ x hx h₁ h₂
    exact .force ((fr_any hx).symm ▸ ih hc hσ any (h₁ ∘ .force) (h₂ ∘ .force))
  | raise b ih =>
    intro hc σ₁ σ₂ hσ x hx h₁ h₂
    exact .raise ((fr_any hx).symm ▸ ih hc hσ any (h₁ ∘ .raise) (h₂ ∘ .raise))
  | scope b ih =>
    intro hc σ₁ σ₂ hσ x _ h₁ h₂
    exact .scope (ih hc (hσ.lift 1) any (h₁ ∘ .scope) (h₂ ∘ .scope))
  | proj b l ih =>
    intro hc σ₁ σ₂ hσ x hx h₁ h₂
    exact .proj ((fr_any hx).symm ▸ ih hc hσ any (h₁ ∘ .proj) (h₂ ∘ .proj))
  | drec c r ih =>
    intro hc σ₁ σ₂ hσ x _ h₁ h₂
    exact .drec (ih hc hσ top (h₁ ∘ .drec) (h₂ ∘ .drec))
      (Tm.NoIns.subst (Tm.Src.noIns (e := r) hc) hσ.noIns)
  | app f a ih₁ ih₂ =>
    intro hc σ₁ σ₂ hσ x hx h₁ h₂
    exact .app hx (ih₁ hc.1 hσ hx (h₁ ∘ .appL) (h₂ ∘ .appL))
      (ih₂ hc.2 hσ hx (h₁ ∘ .appR) (h₂ ∘ .appR))
  | let_ e b ih₁ ih₂ =>
    intro hc σ₁ σ₂ hσ x hx h₁ h₂
    exact .let_ ((fr_any hx).symm ▸ ih₁ hc.1 hσ any (h₁ ∘ .letL) (h₂ ∘ .letL))
      (ih₂ hc.2 (hσ.lift 1) any (h₁ ∘ .letR) (h₂ ∘ .letR))
  | letrec f b ih₁ ih₂ =>
    intro hc σ₁ σ₂ hσ x _ h₁ h₂
    exact .letrec (ih₁ hc.1 (hσ.lift 2) any (h₁ ∘ .letrecL) (h₂ ∘ .letrecL))
      (Tm.NoIns.subst hc.1.noIns (hσ.lift 2).noIns)
      (ih₂ hc.2 (hσ.lift 1) any (h₁ ∘ .letrecR) (h₂ ∘ .letrecR))
  | rcons l e r ih₁ ih₂ =>
    intro hc σ₁ σ₂ hσ x hx h₁ h₂
    exact .rcons hx (ih₁ hc.1 hσ hx (h₁ ∘ .rconsL) (h₂ ∘ .rconsL))
      (ih₂ hc.2 hσ hx (h₁ ∘ .rconsR) (h₂ ∘ .rconsR))
  | update e l a ih₁ ih₂ =>
    intro hc σ₁ σ₂ hσ x hx h₁ h₂
    exact .update hx (ih₁ hc.1 hσ any (h₁ ∘ .updL) (h₂ ∘ .updL))
      (ih₂ hc.2 hσ any (h₁ ∘ .updR) (h₂ ∘ .updR))
  | match_ e al ih₁ ih₂ =>
    intro hc σ₁ σ₂ hσ x hx h₁ h₂
    exact .match_ ((fr_any hx).symm ▸ ih₁ hc.1 hσ any (h₁ ∘ .matchL) (h₂ ∘ .matchL))
      (ih₂ hc.2 hσ top (h₁ ∘ .matchR) (h₂ ∘ .matchR))
  | acons p b r ihb ihr =>
    intro hc σ₁ σ₂ hσ x hx h₁ h₂
    by_cases hd : (b.subst (liftS p.binds σ₁)).LzTop .any ∨ (b.subst (liftS p.binds σ₂)).LzTop .any
    · exact .aconsD hd (Tm.NoIns.subst hc.1.noIns (hσ.lift p.binds).noIns)
        (ihr hc.2 hσ hx (h₁ ∘ .acons) (h₂ ∘ .acons))
    · exact .acons (ihb hc.1 (hσ.lift p.binds) any (fun h => hd (.inl h)) (fun h => hd (.inr h)))
        (ihr hc.2 hσ hx (h₁ ∘ .acons) (h₂ ∘ .acons))
  | aguard p g b r ih₁ ih₂ ih₃ =>
    intro hc σ₁ σ₂ hσ x hx h₁ h₂
    exact .aguard (ih₁ hc.1 (hσ.lift p.binds) any (h₁ ∘ .aguardG) (h₂ ∘ .aguardG))
      (ih₂ hc.2.1 (hσ.lift p.binds) any (h₁ ∘ .aguardB) (h₂ ∘ .aguardB))
      (ih₃ hc.2.2 hσ hx (h₁ ∘ .aguardR) (h₂ ∘ .aguardR))
  | try_ e al ih₁ ih₂ =>
    intro hc σ₁ σ₂ hσ x hx h₁ h₂
    exact .try_ ((fr_any hx).symm ▸ ih₁ hc.1 hσ any (h₁ ∘ .tryL) (h₂ ∘ .tryL))
      (ih₂ hc.2 hσ top (h₁ ∘ .tryR) (h₂ ∘ .tryR))
  | handle v al ih₁ ih₂ =>
    intro hc σ₁ σ₂ hσ x _ h₁ h₂
    exact .handle (ih₁ hc.1 hσ top (h₁ ∘ .handleV) (h₂ ∘ .handleV))
      (ih₂ hc.2 hσ top (h₁ ∘ .handleA) (h₂ ∘ .handleA))
  | hdl E al r b ih₁ ih₂ ih₃ =>
    intro hc σ₁ σ₂ hσ x _ h₁ h₂
    exact .hdl (ih₁ hc.1 hσ top (h₁ ∘ .hdlA) (h₂ ∘ .hdlA))
      (ih₂ hc.2.1 hσ any (h₁ ∘ .hdlR) (h₂ ∘ .hdlR))
      (ih₃ hc.2.2 hσ any (h₁ ∘ .hdlE) (h₂ ∘ .hdlE))
  | _ => intro hc; exact hc.elim

end Flick.Equiv
