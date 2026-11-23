import Formal.Equiv.ActBase

/-!
# Contextual equivalence: raise/unw steps of special pairs
-/

namespace Flick.Equiv

open Flick.Elab (raise_redex unw_redex perform_redex kont_redex fin_redex)

variable {W : World}

/-- A step at a raised error in frame `F` is a runtime head step of `F.plug (.raise v)` or `Step.raise`. -/
theorem Step.at_raise {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' : List Tm}
    {E₁ : List Frame} {F : Frame} {v t' : Tm} (hE₁ : OkE E₁) (hF : F.ok) (hv : Value v)
    (h : Step root l Λ μ (plug E₁ (F.plug (.raise v))) (.cfg Λ' μ' t')) :
    (∃ r', RHead Λ μ (F.plug (.raise v)) Λ' μ' r' ∧ t' = plug E₁ r') ∨
      (¬ F.isTry ∧ ¬ F.isInside ∧ ¬ F.isCtl ∧ Λ' = Λ ∧ μ' = μ ∧ t' = plug E₁ (.raise v)) := by
  have hE : OkE (F :: E₁) := hE₁.cons' hF
  have hr : Redex (.raise v) := raise_redex hv
  have bad : ∀ {E' : List Frame} {r' : Tm}, OkE E' → Redex r' → (∀ w, r' ≠ .raise w) →
      plug E' r' = plug (F :: E₁) (.raise v) → False := fun hE' hr' hne he => by
    obtain ⟨-, rfl⟩ := decomp_eq hr' hr hE' hE he; exact hne v rfl
  change Step root l Λ μ (plug (F :: E₁) (.raise v)) (.cfg Λ' μ' t') at h
  generalize ht : plug (F :: E₁) (.raise v) = t at h
  cases h with
  | pure hE' hh => exact (bad hE' hh.redex (fun _ he => by subst he; cases hh) ht.symm).elim
  | rt hE' hh =>
    rcases hh.redex' with h' | ⟨G, w, hG, hw, rfl⟩ | ⟨G, q, ℓ, u, hG, rfl⟩
    · exact (bad hE' h' (fun _ he => by subst he; cases hh) ht.symm).elim
    · obtain ⟨hEq, hrEq⟩ := decomp_eq (E := G :: _) (raise_redex hw) hr (hE'.cons' hG) hE ht.symm
      cases hEq; cases hrEq
      exact .inl ⟨_, hh, rfl⟩
    · exact (bad (E' := G :: _) (hE'.cons' hG) (unw_redex _ _ _) (fun _ h => by cases h) ht.symm).elim
  | addFin hE' _ _ hv' => exact (bad hE' (fin_redex (.inl rfl) hv') (fun _ h => by cases h) ht.symm).elim
  | spawn hE' _ _ hv' => exact (bad hE' (fin_redex (.inr rfl) hv') (fun _ h => by cases h) ht.symm).elim
  | performRoot hE' _ _ _ hv' => exact (bad hE' (perform_redex hv') (fun _ h => by cases h) ht.symm).elim
  | raise hE' hG hnt hni hnc hv' =>
    obtain ⟨hEq, hrEq⟩ := decomp_eq (E := _ :: _) (raise_redex hv') hr (hE'.cons' hG) hE ht.symm
    cases hEq; cases hrEq
    exact .inr ⟨hnt, hni, hnc, rfl, rfl, rfl⟩
  | unw hE' hG => exact (bad (E' := _ :: _) (hE'.cons' hG) (unw_redex _ _ _) (fun _ h => by cases h) ht.symm).elim
  | performAbort hE' hEi _ _ hv' | performTail hE' hEi _ _ hv' | performCtl hE' hEi _ _ hv' =>
    exact (bad (hEi.append' (hE'.cons' (F := .hwith _ _ _ _ _) trivial)) (perform_redex hv')
      (fun _ h => by cases h) ((plug_append _ _ _).trans ht.symm)).elim
  | performKept hE' hEi _ _ hv' _ =>
    exact (bad (hEi.append' (hE'.cons' (F := .hkeep _ _ _ _) trivial)) (perform_redex hv')
      (fun _ h => by cases h) ((plug_append _ _ _).trans ht.symm)).elim
  | resume hE' hA hEi _ _ hu =>
    exact (bad (hA.append' (hE'.cons' (F := .hctlA _ _ _ _ _ _ _ _) trivial)) (kont_redex hu)
      (fun _ h => by cases h) ((plug_append _ _ _).trans ht.symm)).elim
  | glob hE' _ => exact (bad hE' (glob_redex _) (fun _ h => by cases h) ht.symm).elim

/-- A step at a signal in frame `F` is a runtime head step of `F.plug (.unw p ℓ u)` or `Step.unw`. -/
theorem Step.at_unw {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' : List Tm}
    {E₁ : List Frame} {F : Frame} {p ℓ : Nat} {u t' : Tm} (hE₁ : OkE E₁) (hF : F.ok)
    (h : Step root l Λ μ (plug E₁ (F.plug (.unw p ℓ u))) (.cfg Λ' μ' t')) :
    (∃ r', RHead Λ μ (F.plug (.unw p ℓ u)) Λ' μ' r' ∧ t' = plug E₁ r') ∨
      (¬ F.isInside ∧ ¬ F.isCtl ∧ ¬ F.catches p ℓ ∧ Λ' = Λ ∧ μ' = μ ∧ t' = plug E₁ (.unw p ℓ u)) := by
  have hE : OkE (F :: E₁) := hE₁.cons' hF
  have hr : Redex (.unw p ℓ u) := unw_redex p ℓ u
  have bad : ∀ {E' : List Frame} {r' : Tm}, OkE E' → Redex r' → (∀ q ℓ' w, r' ≠ .unw q ℓ' w) →
      plug E' r' = plug (F :: E₁) (.unw p ℓ u) → False := fun hE' hr' hne he => by
    obtain ⟨-, rfl⟩ := decomp_eq hr' hr hE' hE he; exact hne p ℓ u rfl
  change Step root l Λ μ (plug (F :: E₁) (.unw p ℓ u)) (.cfg Λ' μ' t') at h
  generalize ht : plug (F :: E₁) (.unw p ℓ u) = t at h
  cases h with
  | pure hE' hh => exact (bad hE' hh.redex (fun _ _ _ he => by subst he; cases hh) ht.symm).elim
  | rt hE' hh =>
    rcases hh.redex' with h' | ⟨G, w, hG, hw, rfl⟩ | ⟨G, q, ℓ', w, hG, rfl⟩
    · exact (bad hE' h' (fun _ _ _ he => by subst he; cases hh) ht.symm).elim
    · exact (bad (E' := G :: _) (hE'.cons' hG) (raise_redex hw) (fun _ _ _ h => by cases h) ht.symm).elim
    · obtain ⟨hEq, hrEq⟩ := decomp_eq (E := G :: _) (unw_redex q ℓ' w) hr (hE'.cons' hG) hE ht.symm
      cases hEq; cases hrEq
      exact .inl ⟨_, hh, rfl⟩
  | addFin hE' _ _ hv' => exact (bad hE' (fin_redex (.inl rfl) hv') (fun _ _ _ h => by cases h) ht.symm).elim
  | spawn hE' _ _ hv' => exact (bad hE' (fin_redex (.inr rfl) hv') (fun _ _ _ h => by cases h) ht.symm).elim
  | performRoot hE' _ _ _ hv' => exact (bad hE' (perform_redex hv') (fun _ _ _ h => by cases h) ht.symm).elim
  | raise hE' hG _ _ _ hv' =>
    exact (bad (E' := _ :: _) (hE'.cons' hG) (raise_redex hv') (fun _ _ _ h => by cases h) ht.symm).elim
  | unw hE' hG hni hnc hnh =>
    obtain ⟨hEq, hrEq⟩ := decomp_eq (E := _ :: _) (unw_redex _ _ _) hr (hE'.cons' hG) hE ht.symm
    cases hEq; cases hrEq
    exact .inr ⟨hni, hnc, hnh, rfl, rfl, rfl⟩
  | performAbort hE' hEi _ _ hv' | performTail hE' hEi _ _ hv' | performCtl hE' hEi _ _ hv' =>
    exact (bad (hEi.append' (hE'.cons' (F := .hwith _ _ _ _ _) trivial)) (perform_redex hv')
      (fun _ _ _ h => by cases h) ((plug_append _ _ _).trans ht.symm)).elim
  | performKept hE' hEi _ _ hv' _ =>
    exact (bad (hEi.append' (hE'.cons' (F := .hkeep _ _ _ _) trivial)) (perform_redex hv')
      (fun _ _ _ h => by cases h) ((plug_append _ _ _).trans ht.symm)).elim
  | resume hE' hA hEi _ _ hu =>
    exact (bad (hA.append' (hE'.cons' (F := .hctlA _ _ _ _ _ _ _ _) trivial)) (kont_redex hu)
      (fun _ _ _ h => by cases h) ((plug_append _ _ _).trans ht.symm)).elim
  | glob hE' _ => exact (bad hE' (glob_redex _) (fun _ _ _ h => by cases h) ht.symm).elim

theorem step_settle_raise {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' : List Tm}
    {E₁ : List Frame} {l₀ : Nat} {v b t' : Tm} (hE₁ : OkE E₁) (hv : Value v)
    (h : Step root l Λ μ (plug E₁ (.let_ (.settle l₀ (.raise v)) b)) (.cfg Λ' μ' t')) :
    Λ' = Λ ∧ μ' = settleCell μ l₀ (.raise v) ∧ t' = plug E₁ (.let_ (.raise v) b) := by
  rcases Step.at_raise (E₁ := .let_ b :: E₁) (F := .settle l₀) (hE₁.cons' trivial) trivial hv h with
    ⟨_, hh, rfl⟩ | ⟨-, -, hnc, -⟩
  · cases hh with
    | settleVal hv' => cases hv'
    | settleRaise => exact ⟨rfl, rfl, rfl⟩
  · exact (hnc trivial).elim

theorem step_settle_unw {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' : List Tm}
    {E₁ : List Frame} {l₀ p ℓ : Nat} {u b t' : Tm} (hE₁ : OkE E₁)
    (h : Step root l Λ μ (plug E₁ (.let_ (.settle l₀ (.unw p ℓ u)) b)) (.cfg Λ' μ' t')) :
    Λ' = Λ ∧ μ' = resetCell μ l₀ ∧ t' = plug E₁ (.let_ (.unw p ℓ u) b) := by
  rcases Step.at_unw (E₁ := .let_ b :: E₁) (F := .settle l₀) (hE₁.cons' trivial) trivial h with
    ⟨_, hh, rfl⟩ | ⟨-, hnc, -⟩
  · cases hh with
    | settleVal hv' => cases hv'
    | settleUnw => exact ⟨rfl, rfl, rfl⟩
  · exact (hnc trivial).elim

theorem step_let_raise {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' : List Tm}
    {E₁ : List Frame} {v b t' : Tm} (hE₁ : OkE E₁) (hv : Value v)
    (h : Step root l Λ μ (plug E₁ (.let_ (.raise v) b)) (.cfg Λ' μ' t')) :
    Λ' = Λ ∧ μ' = μ ∧ t' = plug E₁ (.raise v) := by
  rcases Step.at_raise (E₁ := E₁) (F := .let_ b) hE₁ trivial hv h with
    ⟨_, hh, -⟩ | ⟨-, -, -, rfl, rfl, rfl⟩
  · cases hh
  · exact ⟨rfl, rfl, rfl⟩

theorem step_let_unw {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' : List Tm}
    {E₁ : List Frame} {p ℓ : Nat} {u b t' : Tm} (hE₁ : OkE E₁)
    (h : Step root l Λ μ (plug E₁ (.let_ (.unw p ℓ u) b)) (.cfg Λ' μ' t')) :
    Λ' = Λ ∧ μ' = μ ∧ t' = plug E₁ (.unw p ℓ u) := by
  rcases Step.at_unw (E₁ := E₁) (F := .let_ b) hE₁ trivial h with
    ⟨_, hh, -⟩ | ⟨-, -, -, rfl, rfl, rfl⟩
  · cases hh
  · exact ⟨rfl, rfl, rfl⟩

/-- When a raised error inside a non-control frame `F` sits inside a special pair `Act`, the pair is
`letRaise` (or `o4Settle`, ruled out by `PureEval`). -/
theorem Act.eq_raise {μ μ₂ : List Tm} (hst : StR W μ μ₂) {a b v : Tm} {F : Frame}
    {E E₀ E₁ : List Frame} (hact : Act W a b) (hv : Value v) (hnc : ¬ F.isCtl)
    (hE : OkE E₀) (hEq : F :: E = E₀ ++ E₁) (ha : a = plug E₀ (.raise v)) :
    (∃ v' b₁ b₂, E₀ = [.let_ b₁] ∧ E₁ = E ∧ F = .let_ b₁ ∧ b = .let_ (.raise v') b₂ ∧
      Value v' ∧ Sim W .any v v') ∨
    (∃ l l' e t, a = .settle l t ∧ W.H l l' ∧ W.HL l e ∧ PSteps W.R e t ∧ t.Closed) := by
  have val : ∀ {l l' w w'}, W.L l l' (.born w w') → Value w := fun hl => by
    obtain ⟨c, c', -, -, hi⟩ := hst.cell _ _ _ hl
    exact (show BornInv W _ _ c c' from hi).val
  have bad : ∀ {A : List Frame} {r : Tm}, OkE A → Redex r → (∀ w, r ≠ .raise w) →
      a = plug A r → False := fun hA hr hne he => by
    obtain ⟨-, rfl⟩ := decomp_eq hr (raise_redex hv) hA hE (he.symm.trans ha)
    exact hne v rfl
  cases hact with
  | dropA => exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) (lazy_redex _) (fun _ h => by cases h) rfl).elim
  | dropB => exact (bad (A := []) OkE.nil (let_redex .loc) (fun _ h => by cases h) rfl).elim
  | forceA => exact (bad (A := [.force]) (OkE.nil.cons' trivial) (lazy_redex _) (fun _ h => by cases h) rfl).elim
  | bornA => exact (bad (A := []) OkE.nil (lazy_redex _) (fun _ h => by cases h) rfl).elim
  | bornB => exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) (lazy_redex _) (fun _ h => by cases h) rfl).elim
  | bornV _ hl => exact (bad (A := []) OkE.nil (settle_redex (val hl)) (fun _ h => by cases h) rfl).elim
  | dead => exact (bad (A := []) OkE.nil todo_redex (fun _ h => by cases h) rfl).elim
  | o4B | o4Alloc1 => exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) (lazy_redex _) (fun _ h => by cases h) rfl).elim
  | o4Bind => exact (bad (A := []) OkE.nil (let_redex .loc) (fun _ h => by cases h) rfl).elim
  | o4Settle _ hH hhl _ hct hpt => exact .inr ⟨_, _, _, _, rfl, hH, hhl, hpt, hct⟩
  | doneF => exact (bad (A := []) OkE.nil (force_redex _) (fun _ h => by cases h) rfl).elim
  | againF1 _ _ _ hve _ _ _ _ _ | againR1 _ _ _ hve _ _ _ _ _ =>
    rcases hve with ⟨j, rfl⟩ | hv'
    · exact (bad (A := [.force, .let_ _]) ((OkE.nil.cons' (F := .let_ _) trivial).cons' (F := .force) trivial) (var_redex j) (fun _ h => by cases h) rfl).elim
    · exact (bad (A := [.let_ _]) (OkE.nil.cons' (F := .let_ _) trivial) (force_val_redex hv') (fun _ h => by cases h) rfl).elim
  | againF2 _ _ hfn _ _ _ _ _ _ | againR2 _ _ hfn _ _ _ _ _ _ =>
    rcases hfn with hv' | ⟨w, hw, rfl⟩ | ⟨p, ℓ, u, rfl⟩
    · exact (bad (A := [.let_ _]) (OkE.nil.cons' (F := .let_ _) trivial) (settle_redex hv') (fun _ h => by cases h) rfl).elim
    · obtain ⟨hE₀, -⟩ := decomp_eq (E := [.settle _, .let_ _]) (raise_redex hw) (raise_redex hv)
        ((OkE.nil.cons' (F := .let_ _) trivial).cons' (F := .settle _) trivial) hE ha
      subst hE₀; cases hEq; exact (hnc trivial).elim
    · exact (bad (A := [.settle _, .let_ _]) ((OkE.nil.cons' (F := .let_ _) trivial).cons' (F := .settle _) trivial) (unw_redex p ℓ u) (fun _ h => by cases h) rfl).elim
  | againF3 _ hd | againR3 _ hd =>
    exact (bad (A := []) OkE.nil (let_redex (hst.done _ _ _ _ hd).2.1) (fun _ h => by cases h) rfl).elim
  | letRaise _ hv₀ hv' hvv =>
    obtain ⟨hE₀, hrEq⟩ := decomp_eq (E := [.let_ _]) (raise_redex hv₀) (raise_redex hv)
      (OkE.nil.cons' trivial) hE ha
    subst hE₀; cases hrEq; cases hEq
    exact .inl ⟨_, _, _, rfl, rfl, rfl, rfl, hv', hvv⟩
  | letUnw => exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) (unw_redex _ _ _) (fun _ h => by cases h) rfl).elim
  | letTodo => exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) todo_redex (fun _ h => by cases h) rfl).elim
  | bornS _ hl _ hs =>
    rcases hs with rfl | rfl | rfl | rfl | rfl
    · exact (bad (A := []) OkE.nil (let_redex .loc) (fun _ h => by cases h) rfl).elim
    · exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) (force_redex _) (fun _ h => by cases h) rfl).elim
    · exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) (settle_redex (val hl)) (fun _ h => by cases h) rfl).elim
    · exact (bad (A := []) OkE.nil (let_redex (val hl)) (fun _ h => by cases h) rfl).elim
    · exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) todo_redex (fun _ h => by cases h) rfl).elim

/-- When a signal inside a non-control frame `F` sits inside a special pair `Act`, the pair is
`letUnw` (or `o4Settle`, ruled out by `PureEval`). -/
theorem Act.eq_unw {μ μ₂ : List Tm} (hst : StR W μ μ₂) {a b u : Tm} {p ℓ : Nat} {F : Frame}
    {E E₀ E₁ : List Frame} (hact : Act W a b) (hnc : ¬ F.isCtl)
    (hE : OkE E₀) (hEq : F :: E = E₀ ++ E₁) (ha : a = plug E₀ (.unw p ℓ u)) :
    (∃ ℓ' k u' b₁ b₂, E₀ = [.let_ b₁] ∧ E₁ = E ∧ F = .let_ b₁ ∧ b = .let_ (.unw p ℓ' u') b₂ ∧
      W.L ℓ ℓ' k ∧ Sim W .any u u') ∨
    (∃ l l' e t, a = .settle l t ∧ W.H l l' ∧ W.HL l e ∧ PSteps W.R e t ∧ t.Closed) := by
  have val : ∀ {l l' w w'}, W.L l l' (.born w w') → Value w := fun hl => by
    obtain ⟨c, c', -, -, hi⟩ := hst.cell _ _ _ hl
    exact (show BornInv W _ _ c c' from hi).val
  have bad : ∀ {A : List Frame} {r : Tm}, OkE A → Redex r → (∀ q ℓ' w, r ≠ .unw q ℓ' w) →
      a = plug A r → False := fun hA hr hne he => by
    obtain ⟨-, rfl⟩ := decomp_eq hr (unw_redex p ℓ u) hA hE (he.symm.trans ha)
    exact hne p ℓ u rfl
  cases hact with
  | dropA => exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) (lazy_redex _) (fun _ _ _ h => by cases h) rfl).elim
  | dropB => exact (bad (A := []) OkE.nil (let_redex .loc) (fun _ _ _ h => by cases h) rfl).elim
  | forceA => exact (bad (A := [.force]) (OkE.nil.cons' trivial) (lazy_redex _) (fun _ _ _ h => by cases h) rfl).elim
  | bornA => exact (bad (A := []) OkE.nil (lazy_redex _) (fun _ _ _ h => by cases h) rfl).elim
  | bornB => exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) (lazy_redex _) (fun _ _ _ h => by cases h) rfl).elim
  | bornV _ hl => exact (bad (A := []) OkE.nil (settle_redex (val hl)) (fun _ _ _ h => by cases h) rfl).elim
  | dead => exact (bad (A := []) OkE.nil todo_redex (fun _ _ _ h => by cases h) rfl).elim
  | o4B | o4Alloc1 => exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) (lazy_redex _) (fun _ _ _ h => by cases h) rfl).elim
  | o4Bind => exact (bad (A := []) OkE.nil (let_redex .loc) (fun _ _ _ h => by cases h) rfl).elim
  | o4Settle _ hH hhl _ hct hpt => exact .inr ⟨_, _, _, _, rfl, hH, hhl, hpt, hct⟩
  | doneF => exact (bad (A := []) OkE.nil (force_redex _) (fun _ _ _ h => by cases h) rfl).elim
  | againF1 _ _ _ hve _ _ _ _ _ | againR1 _ _ _ hve _ _ _ _ _ =>
    rcases hve with ⟨j, rfl⟩ | hv'
    · exact (bad (A := [.force, .let_ _]) ((OkE.nil.cons' (F := .let_ _) trivial).cons' (F := .force) trivial) (var_redex j) (fun _ _ _ h => by cases h) rfl).elim
    · exact (bad (A := [.let_ _]) (OkE.nil.cons' (F := .let_ _) trivial) (force_val_redex hv') (fun _ _ _ h => by cases h) rfl).elim
  | againF2 _ _ hfn _ _ _ _ _ _ | againR2 _ _ hfn _ _ _ _ _ _ =>
    rcases hfn with hv' | ⟨w, hw, rfl⟩ | ⟨q, ℓ', w, rfl⟩
    · exact (bad (A := [.let_ _]) (OkE.nil.cons' (F := .let_ _) trivial) (settle_redex hv') (fun _ _ _ h => by cases h) rfl).elim
    · exact (bad (A := [.settle _, .let_ _]) ((OkE.nil.cons' (F := .let_ _) trivial).cons' (F := .settle _) trivial) (raise_redex hw) (fun _ _ _ h => by cases h) rfl).elim
    · obtain ⟨hE₀, -⟩ := decomp_eq (E := [.settle _, .let_ _]) (unw_redex q ℓ' w) (unw_redex p ℓ u)
        ((OkE.nil.cons' (F := .let_ _) trivial).cons' (F := .settle _) trivial) hE ha
      subst hE₀; cases hEq; exact (hnc trivial).elim
  | againF3 _ hd | againR3 _ hd =>
    exact (bad (A := []) OkE.nil (let_redex (hst.done _ _ _ _ hd).2.1) (fun _ _ _ h => by cases h) rfl).elim
  | letRaise _ hv₀ => exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) (raise_redex hv₀) (fun _ _ _ h => by cases h) rfl).elim
  | letUnw _ hl hu =>
    obtain ⟨hE₀, hrEq⟩ := decomp_eq (E := [.let_ _]) (unw_redex _ _ _) (unw_redex p ℓ u)
      (OkE.nil.cons' trivial) hE ha
    subst hE₀; cases hrEq; cases hEq
    exact .inl ⟨_, _, _, _, _, rfl, rfl, rfl, rfl, hl, hu⟩
  | letTodo => exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) todo_redex (fun _ _ _ h => by cases h) rfl).elim
  | bornS _ hl _ hs =>
    rcases hs with rfl | rfl | rfl | rfl | rfl
    · exact (bad (A := []) OkE.nil (let_redex .loc) (fun _ _ _ h => by cases h) rfl).elim
    · exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) (force_redex _) (fun _ _ _ h => by cases h) rfl).elim
    · exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) (settle_redex (val hl)) (fun _ _ _ h => by cases h) rfl).elim
    · exact (bad (A := []) OkE.nil (let_redex (val hl)) (fun _ _ _ h => by cases h) rfl).elim
    · exact (bad (A := [.let_ _]) (OkE.nil.cons' trivial) todo_redex (fun _ _ _ h => by cases h) rfl).elim

/-- The left side of a special pair steps at its redex `r` in `F`. -/
theorem step_in {root : Roots} {l : Lbl} {Λ Λ' : List Rgn} {μ μ' : List Tm} {E₁ : List Frame}
    {F : Frame} {r t' : Tm} (hE₁ : OkE E₁) (hF : F.ok) (hr : Redex r) (hs : r.Simple)
    (h : Step root l Λ μ (plug E₁ (F.plug r)) (.cfg Λ' μ' t')) :
    (∃ r', PHead r r' ∧ Λ' = Λ ∧ μ' = μ ∧ t' = plug E₁ (F.plug r')) ∨
      (∃ r', RHead Λ μ r Λ' μ' r' ∧ t' = plug E₁ (F.plug r')) :=
  Step.simple (E := F :: E₁) (hE₁.cons' hF) hr hs h

theorem born_closed {v : Tm} (h : (Tm.let_ (.lazy v) (.let_ (.force (.var 0)) (.var 1))).Closed) :
    v.Closed := fun i hi => h i (by simp only [Tm.Free]; exact Or.inl hi)

theorem lazy_closed {v : Tm} (h : (Tm.lazy v).Closed) : v.Closed := fun i hi => h i hi

theorem append_set_last {μ : List Tm} {c c' : Tm} : (μ ++ [c]).set μ.length c' = μ ++ [c'] := by
  simp

end Flick.Equiv
