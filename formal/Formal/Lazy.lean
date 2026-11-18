import Formal.Effects
import Formal.Elab.CoreRel

/-!
# The laws of a lazy cell (lazy.md §3)

The terms, types and steps of lazy cells are those of the core: `Ty.lazy` and the store type
`Ty.lazyCell` (`TyBase`), `Tm.lazy`, `Tm.force` and the frame `Tm.settle` (`Syntax`), their
typing (`Rules`, `StoreTy`) and their steps (`Semantics`); preservation and progress cover them
(`PresLazy`, `ProgressMain`). Here are the two laws of one thread.

**L1 (once per attempt).** On any path the starts and the resets of a cell alternate
(`Lazy.L1`): between two resets the initializer of a cell starts at most once
(`Lazy.L1_once`). The only step that starts it is the force of the pending cell, which puts the
initializer in the frame `settle ℓ` and leaves the cell running with it (`Starts`); the only step
that makes a cell pending again is a reset (`Step.pending`): a signal of a handler — an `abort`,
or a `control` arm that lets the body go — leaves the frame `settle ℓ` of the running cell, which
gets its initializer back (`Resets`). A raised error and a value are outcomes and settle it.

**L2 (final).** On any path from a typed configuration a settled cell keeps its outcome, and
a later force of it steps to the term of that outcome and leaves the store (`Lazy.L2`). The
write of an outcome is guarded by the state `Running` (`settleCell`), which is the state of
the cell at every `settle ℓ` a program reaches; with it, the law holds of every typed
configuration.
-/

namespace Flick

variable {S : Sig}

/-- The cell `ℓ` is pending. -/
def Pending (μ : List Tm) (l : Nat) : Prop := ∃ e, μ[l]? = some (.lazy e)

/-- The cell `ℓ` is running. -/
def Running (μ : List Tm) (l : Nat) : Prop := ∃ e, μ[l]? = some (.running e)

/-- A step that starts the initializer of the cell `ℓ`: it forces the pending cell, puts the
initializer in the frame `settle ℓ`, and leaves the cell running with it. -/
def Starts (l : Nat) (μ : List Tm) (e : Tm) (μ' : List Tm) (e' : Tm) : Prop :=
  ∃ E x, OkE E ∧ μ[l]? = some (.lazy x) ∧ e = plug E (.force (.loc l)) ∧
    e' = plug E (.settle l x) ∧ μ' = μ.set l (.running x)

/-- A step that resets the cell `ℓ`: a signal leaves the frame `settle ℓ` of the running cell,
which is pending again with its initializer. -/
def Resets (l : Nat) (μ : List Tm) (e : Tm) (μ' : List Tm) (e' : Tm) : Prop :=
  ∃ E x p ℓ a, OkE E ∧ μ[l]? = some (.running x) ∧ e = plug E (.settle l (.unw p ℓ a)) ∧
    e' = plug E (.unw p ℓ a) ∧ μ' = μ.set l (.lazy x)

/-- What a step does to the store: nothing, a new cell that is not running, a value written into
a cell, the start of an initializer, the reset of a running cell, or an outcome written into a
running cell. -/
theorem Step.cells {root lbl Λ μ e Λ' μ' e'} (h : Step root lbl Λ μ e (.cfg Λ' μ' e')) :
    μ' = μ ∨ (∃ c, μ' = μ ++ [c] ∧ ∀ x, c ≠ .running x) ∨ (∃ l v, Value v ∧ μ' = μ.set l v) ∨
      (∃ l, Starts l μ e μ' e') ∨ (∃ l, Resets l μ e μ' e') ∨ ∃ l o, μ' = settleCell μ l o := by
  cases h with
  | rt hE hh =>
    cases hh with
    | refMake hv => exact .inr (.inl ⟨_, rfl, fun _ h => by subst h; cases hv⟩)
    | lazy | hdl => exact .inr (.inl ⟨_, rfl, fun _ h => by cases h⟩)
    | refSet hv => exact .inr (.inr (.inl ⟨_, _, hv, rfl⟩))
    | forcePending hl => exact .inr (.inr (.inr (.inl ⟨_, _, _, hE, hl, rfl, rfl, rfl⟩)))
    | settleUnw =>
      unfold resetCell
      split
      · rename_i x hx
        exact .inr (.inr (.inr (.inr (.inl ⟨_, _, x, _, _, _, hE, hx, rfl, rfl, rfl⟩))))
      · exact .inl rfl
    | settleVal | settleRaise => exact .inr (.inr (.inr (.inr (.inr ⟨_, _, rfl⟩))))
    | _ => exact .inl rfl
  | performCtl => exact .inr (.inl ⟨_, rfl, fun _ h => by cases h⟩)
  | _ => exact .inl rfl

theorem Step.length_le {root lbl Λ μ e Λ' μ' e'} (h : Step root lbl Λ μ e (.cfg Λ' μ' e')) :
    μ.length ≤ μ'.length := by
  rcases h.cells with rfl | ⟨_, rfl, -⟩ | ⟨_, _, _, rfl⟩ | ⟨_, _, _, _, _, _, _, rfl⟩ |
    ⟨_, _, _, _, _, _, _, _, _, _, rfl⟩ | ⟨_, _, rfl⟩ <;> simp

/-- **A reset only from a running cell.** A step after which a cell of the store is pending
leaves it pending, or resets it: a signal leaves the frame of the running cell. -/
theorem Step.pending {root lbl Λ μ e Λ' μ' e' l} (h : Step root lbl Λ μ e (.cfg Λ' μ' e'))
    (hl : l < μ.length) (hp : Pending μ' l) : Pending μ l ∨ Resets l μ e μ' e' := by
  obtain ⟨x, hx⟩ := hp
  rcases h.cells with rfl | ⟨_, rfl, -⟩ | ⟨_, _, hv, rfl⟩ | ⟨_, _, _, _, _, _, _, rfl⟩ |
    ⟨l₀, hr⟩ | ⟨_, _, rfl⟩
  · exact .inl ⟨x, hx⟩
  · rw [List.getElem?_append_left hl] at hx; exact .inl ⟨x, hx⟩
  · rcases mem_set_get hx with hx | ⟨-, rfl⟩
    · exact .inl ⟨x, hx⟩
    · cases hv
  · rcases mem_set_get hx with hx | ⟨-, h⟩
    · exact .inl ⟨x, hx⟩
    · cases h
  · have hr₀ := hr
    obtain ⟨_, _, _, _, _, _, _, _, _, rfl⟩ := hr₀
    rcases mem_set_get hx with hx | ⟨rfl, -⟩
    · exact .inl ⟨x, hx⟩
    · exact .inr hr
  · rcases settleCell_get hx with hx | ⟨-, h⟩
    · exact .inl ⟨x, hx⟩
    · cases h

/-- A step after which a cell is running leaves it running, or starts it. -/
theorem Step.running {root lbl Λ μ e Λ' μ' e' l} (h : Step root lbl Λ μ e (.cfg Λ' μ' e'))
    (hr : Running μ' l) : Running μ l ∨ Starts l μ e μ' e' := by
  obtain ⟨x, hx⟩ := hr
  rcases h.cells with rfl | ⟨c, rfl, hc⟩ | ⟨_, _, hv, rfl⟩ | ⟨l₀, hs⟩ |
    ⟨_, _, _, _, _, _, _, _, _, _, rfl⟩ | ⟨_, _, rfl⟩
  · exact .inl ⟨x, hx⟩
  · by_cases hl : l < μ.length
    · rw [List.getElem?_append_left hl] at hx; exact .inl ⟨x, hx⟩
    · rw [List.getElem?_append_right (by omega)] at hx
      cases hj : l - μ.length with
      | zero => simp [hj] at hx; exact (hc x hx).elim
      | succ => simp [hj] at hx
  · rcases mem_set_get hx with hx | ⟨-, rfl⟩
    · exact .inl ⟨x, hx⟩
    · cases hv
  · have hs₀ := hs
    obtain ⟨_, _, _, _, _, _, rfl⟩ := hs₀
    rcases mem_set_get hx with hx | ⟨rfl, -⟩
    · exact .inl ⟨x, hx⟩
    · exact .inr hs
  · rcases mem_set_get hx with hx | ⟨-, h⟩
    · exact .inl ⟨x, hx⟩
    · cases h
  · rcases settleCell_get hx with hx | ⟨-, h⟩
    · exact .inl ⟨x, hx⟩
    · cases h

/-- A path, with the number of its steps that start the initializer of `ℓ` and of those that
reset `ℓ`. -/
inductive StartsN (root : Roots) (l : Nat) :
    List Rgn → List Tm → Tm → List Rgn → List Tm → Tm → Nat → Nat → Prop
  | refl : StartsN root l Λ μ e Λ μ e 0 0
  | start {lb : Lbl} : Step root lb Λ μ e (.cfg Λ₁ μ₁ e₁) → Starts l μ e μ₁ e₁ →
      StartsN root l Λ₁ μ₁ e₁ Λ' μ' e' n m → StartsN root l Λ μ e Λ' μ' e' (n + 1) m
  | reset {lb : Lbl} : Step root lb Λ μ e (.cfg Λ₁ μ₁ e₁) → Resets l μ e μ₁ e₁ →
      StartsN root l Λ₁ μ₁ e₁ Λ' μ' e' n m → StartsN root l Λ μ e Λ' μ' e' n (m + 1)
  | other {lb : Lbl} : Step root lb Λ μ e (.cfg Λ₁ μ₁ e₁) → ¬ Starts l μ e μ₁ e₁ →
      ¬ Resets l μ e μ₁ e₁ → StartsN root l Λ₁ μ₁ e₁ Λ' μ' e' n m →
      StartsN root l Λ μ e Λ' μ' e' n m

theorem StartsN.cons {root : Roots} {l : Nat} {lb : Lbl} {Λ Λ₁ Λ' : List Rgn} {μ μ₁ μ' : List Tm}
    {e e₁ e' : Tm} {n m : Nat} (h₁ : Step root lb Λ μ e (.cfg Λ₁ μ₁ e₁))
    (h : StartsN root l Λ₁ μ₁ e₁ Λ' μ' e' n m) : ∃ n m, StartsN root l Λ μ e Λ' μ' e' n m := by
  by_cases hs : Starts l μ e μ₁ e₁
  · exact ⟨_, _, .start h₁ hs h⟩
  by_cases hr : Resets l μ e μ₁ e₁
  · exact ⟨_, _, .reset h₁ hr h⟩
  · exact ⟨_, _, .other h₁ hs hr h⟩

/-- Every path counts its starts and its resets. -/
theorem Steps.startsN {root Q Λ μ e Λ' μ' e'} (h : Steps root Q Λ μ e Λ' μ' e') (l : Nat) :
    ∃ n m, StartsN root l Λ μ e Λ' μ' e' n m := by
  induction h with
  | refl => exact ⟨0, 0, .refl⟩
  | step h₁ _ _ ih => obtain ⟨n, m, hn⟩ := ih; exact StartsN.cons h₁ hn

theorem Starts.pending {l μ e μ' e'} (h : Starts l μ e μ' e') : Pending μ l ∧ Running μ' l := by
  obtain ⟨_, x, -, hx, -, -, rfl⟩ := h
  exact ⟨⟨x, hx⟩, x, List.getElem?_set_self (getElem?_lt hx)⟩

theorem Resets.running {l μ e μ' e'} (h : Resets l μ e μ' e') : Running μ l ∧ Pending μ' l := by
  obtain ⟨_, x, _, _, _, -, hx, -, -, rfl⟩ := h
  exact ⟨⟨x, hx⟩, x, List.getElem?_set_self (getElem?_lt hx)⟩

theorem not_pending_running {μ : List Tm} {l : Nat} (hp : Pending μ l) (hr : Running μ l) : False := by
  obtain ⟨_, hp⟩ := hp
  obtain ⟨_, hr⟩ := hr
  rw [hp] at hr; cases hr

/-- The count of starts `n` and resets `m` of a path from the store `μ`, by the state of `ℓ`. -/
private def Alt (μ : List Tm) (l n m : Nat) : Prop :=
  (Running μ l → n ≤ m ∧ m ≤ n + 1) ∧ (¬ Running μ l → m ≤ n ∧ n ≤ m + 1) ∧
    (l < μ.length → ¬ Running μ l → ¬ Pending μ l → n = 0 ∧ m = 0)

private theorem Alt.other {root lb Λ μ e Λ₁ μ₁ e₁ l n m}
    (h₁ : Step root lb Λ μ e (.cfg Λ₁ μ₁ e₁)) (hs : ¬ Starts l μ e μ₁ e₁)
    (hr : ¬ Resets l μ e μ₁ e₁) (ih : Alt μ₁ l n m) : Alt μ l n m := by
  have keep : ¬ Running μ l → ¬ Running μ₁ l := fun h h' => (h₁.running h').elim h hs
  refine ⟨fun hrun => ?_, fun hn => ih.2.1 (keep hn), fun hl hn hp => ?_⟩
  · by_cases hrun' : Running μ₁ l
    · exact ih.1 hrun'
    · obtain ⟨_, hx⟩ := hrun
      have hl := getElem?_lt hx
      have := ih.2.2 (Nat.lt_of_lt_of_le hl h₁.length_le) hrun' fun hp' =>
        (h₁.pending hl hp').elim (fun hp => not_pending_running hp ⟨_, hx⟩) hr
      omega
  · exact ih.2.2 (Nat.lt_of_lt_of_le hl h₁.length_le) (keep hn) fun hp' =>
      (h₁.pending hl hp').elim hp hr

private theorem startsN_alt {root l Λ μ e Λ' μ' e' n m} (h : StartsN root l Λ μ e Λ' μ' e' n m) :
    Alt μ l n m := by
  induction h with
  | refl => exact ⟨fun _ => by omega, fun _ => by omega, fun _ _ _ => by omega⟩
  | start _ hs _ ih =>
    obtain ⟨hp, hr⟩ := hs.pending
    have := ih.1 hr
    exact ⟨fun h => (not_pending_running hp h).elim, fun _ => by omega,
      fun _ _ h => (h hp).elim⟩
  | reset _ hs _ ih =>
    obtain ⟨hr, hp⟩ := hs.running
    have := ih.2.1 fun h => not_pending_running hp h
    exact ⟨fun _ => by omega, fun h => (h hr).elim, fun _ h => (h hr).elim⟩
  | other h₁ hs hr _ ih => exact Alt.other h₁ hs hr ih

/-- **L1 (once per attempt).** On any path, the starts and the resets of the initializer of a
lazy cell alternate: from a cell that is not running, there are as many resets as starts or one
start more; from a running cell, as many starts as resets or one reset more. A start needs the
cell pending and leaves it running; only a reset makes a cell pending again (`Step.pending`). -/
theorem Lazy.L1 {root l Λ μ e Λ' μ' e' n m} (h : StartsN root l Λ μ e Λ' μ' e' n m) :
    (¬ Running μ l → m ≤ n ∧ n ≤ m + 1) ∧ (Running μ l → n ≤ m ∧ m ≤ n + 1) :=
  ⟨(startsN_alt h).2.1, (startsN_alt h).1⟩

/-- Between two resets, the initializer of a lazy cell starts at most once: a path with no reset
of the cell starts it at most once. -/
theorem Lazy.L1_once {root l Λ μ e Λ' μ' e' n} (h : StartsN root l Λ μ e Λ' μ' e' n 0) :
    n ≤ 1 := by
  by_cases hr : Running μ l
  · have := ((Lazy.L1 h).2 hr).1; omega
  · have := ((Lazy.L1 h).1 hr).2; omega

theorem Value.not_cell {v : Tm} (hv : Value v) : ¬ v.IsCell := by
  cases hv <;> simp [Tm.IsCell]

/-- A step keeps a settled cell of a typed store, given the typing of the store after it. -/
theorem Step.done_stays {root : Roots} {lbl : Lbl} {Ψ X : List Ty} {Λ Λ' : List Rgn}
    {μ μ' : List Tm} {e e' o : Tm} {l : Nat} (hμ : StoreTy S Ψ μ)
    (hμ' : StoreTy S (Ψ ++ X) μ') (h : Step root lbl Λ μ e (.cfg Λ' μ' e'))
    (hd : μ[l]? = some (.done o)) : μ'[l]? = some (.done o) := by
  have hl := getElem?_lt hd
  rcases h.cells with rfl | ⟨_, rfl, -⟩ | ⟨l₀, v, hv, rfl⟩ | ⟨l₀, _, x, -, hx, -, -, rfl⟩ |
    ⟨l₀, _, x, _, _, _, -, hx, -, -, rfl⟩ | ⟨l₀, x, rfl⟩
  · exact hd
  · rw [List.getElem?_append_left hl]; exact hd
  · by_cases he : l₀ = l
    · subst he
      exfalso
      obtain ⟨t, ht⟩ : ∃ t, Ψ[l₀]? = some t := ⟨_, List.getElem?_eq_getElem (hμ.1 ▸ hl)⟩
      rcases hμ.2 _ _ _ hd ht with ⟨hv', -⟩ | ⟨r, ε, a, C, rfl, -⟩
      · cases hv'
      rcases hμ'.2 l₀ v _ (List.getElem?_set_self hl) (by rw [List.getElem?_append_left (getElem?_lt ht)]; exact ht)
        with ⟨-, hv'⟩ | ⟨_, _, _, _, -, hc⟩
      · exact hv'.value_not_cell hv
      · exact hv.not_cell hc.isCell
    · rw [List.getElem?_set_ne he]; exact hd
  · rw [List.getElem?_set_ne fun he => by subst he; rw [hd] at hx; cases hx]; exact hd
  · rw [List.getElem?_set_ne fun he => by subst he; rw [hd] at hx; cases hx]; exact hd
  · unfold settleCell
    split
    · rename_i hr
      obtain ⟨_, hr⟩ := hr
      rw [List.getElem?_set_ne fun he => by subst he; rw [hd] at hr; cases hr]; exact hd
    · exact hd

/-- A force of a settled cell is a redex. -/
theorem force_redex (l : Nat) : Redex (.force (.loc l)) :=
  ⟨fun h => (by cases h), fun F a _ ha => (by cases F <;> simp [Frame.plug] at ha; exact ha ▸ .loc)⟩

theorem todo_redex : Redex .todo :=
  ⟨fun h => (by cases h), fun F a _ ha => (by cases F <;> simp [Frame.plug] at ha)⟩

/-- Every step of a force of a settled cell gives the term of its outcome and leaves the store
and the regions. -/
theorem Step.force_done {root : Roots} {lbl : Lbl} {Λ : List Rgn} {μ : List Tm} {E : List Frame}
    {l : Nat} {o : Tm} {out : Out} (hE : OkE E) (hd : μ[l]? = some (.done o))
    (hs : Step root lbl Λ μ (plug E (.force (.loc l))) out) : out = .cfg Λ μ (plug E o) := by
  have key : ∀ {E' x}, OkE E' → Redex x → plug E' x = plug E (.force (.loc l)) →
      E' = E ∧ x = .force (.loc l) := fun hE' hx h => decomp_eq hx (force_redex l) hE' hE h
  have cons : ∀ {E' : List Frame} {F : Frame}, OkE E' → F.ok → OkE (F :: E') := fun hE' hF =>
    hE'.cons' hF
  have nest : ∀ {E' Ei : List Frame} (F : Frame) {x : Tm}, OkE E' → OkE Ei → F.ok → Redex x →
      plug E' (F.plug (plug Ei x)) = plug E (.force (.loc l)) → x = .force (.loc l) := by
    intro E' Ei F x hE' hEi hF hx h
    have h' : plug (Ei ++ F :: E') x = plug E (.force (.loc l)) := by rw [plug_append]; exact h
    exact (key (hEi.append' (hE'.cons' hF)) hx h').2
  generalize hq : plug E (.force (.loc l)) = q at hs
  cases hs with
  | pure hE' hh => obtain ⟨-, rfl⟩ := key hE' hh.redex hq.symm; cases hh
  | glob hE' => obtain ⟨-, h⟩ := key hE' (glob_redex _) hq.symm; cases h
  | todo hE' => obtain ⟨-, h⟩ := key hE' todo_redex hq.symm; cases h
  | rt hE' hh =>
    rcases hh.redex' with hx | ⟨F, v, hF, hv, rfl⟩ | ⟨F, q, ℓ, a, hF, rfl⟩
    · obtain ⟨rfl, rfl⟩ := key hE' hx hq.symm
      cases hh with
      | forcePending hl => rw [hd] at hl; cases hl
      | forceDone hl => rw [hd] at hl; cases hl; rfl
      | forceRunning hl _ hn => rw [hd] at hl; cases hl; exact (hn _ rfl).elim
    · obtain ⟨-, h⟩ := key (E' := F :: _) (cons hE' hF) (Elab.raise_redex hv) hq.symm; cases h
    · obtain ⟨-, h⟩ := key (E' := F :: _) (cons hE' hF) (Elab.unw_redex q ℓ a) hq.symm; cases h
  | performRoot hE' _ _ _ hv =>
    obtain ⟨-, h⟩ := key hE' (Elab.perform_redex hv) hq.symm; cases h
  | addFin hE' _ _ hv => obtain ⟨-, h⟩ := key hE' (Elab.fin_redex (.inl rfl) hv) hq.symm; cases h
  | spawn hE' _ _ hv => obtain ⟨-, h⟩ := key hE' (Elab.fin_redex (.inr rfl) hv) hq.symm; cases h
  | raise hE' hF _ _ _ hv =>
    obtain ⟨-, h⟩ := key (E' := _ :: _) (cons hE' hF) (Elab.raise_redex hv) hq.symm; cases h
  | unw hE' hF => obtain ⟨-, h⟩ := key (E' := _ :: _) (cons hE' hF) (Elab.unw_redex _ _ _) hq.symm; cases h
  | performAbort hE' hEi _ _ hv | performTail hE' hEi _ _ hv | performCtl hE' hEi _ _ hv =>
    cases nest (.hwith _ _ _ _ _) hE' hEi trivial (Elab.perform_redex hv) hq.symm
  | performKept hE' hEi _ _ hv _ =>
    cases nest (.hkeep _ _ _ _) hE' hEi trivial (Elab.perform_redex hv) hq.symm
  | resume hE' hA _ _ _ hu =>
    cases nest (.hctlA _ _ _ _ _ _ _ _) hE' hA trivial (Elab.kont_redex hu) hq.symm
  | resumeDie hE' hu => obtain ⟨-, h⟩ := key hE' (Elab.kont_redex hu) hq.symm; cases h

/-- **L2 (final).** On any path from a typed configuration, a settled cell keeps its outcome,
and a force of it in a later configuration steps to the term of the outcome — `v`, `raise v` or
`todo` — and leaves the store. -/
theorem Lazy.L2 {root : Roots} {Q : Lbl → Prop} {Ψ : List Ty} {Λ Λ' : List Rgn} {μ μ' : List Tm}
    {e e' o : Tm} {t : Ty} {φ : Stage} {l : Nat} (hroot : RootTy S root) (hμ : StoreTy S Ψ μ)
    (hb : CfgBelow Λ μ e) (hΨ : ∀ u ∈ Ψ, u.YF Λ.length) (h : Typ S Ψ Facts.age [] e (.ex t φ))
    (hs : Steps root Q Λ μ e Λ' μ' e') (hd : μ[l]? = some (.done o)) :
    μ'[l]? = some (.done o) ∧ ∀ E lbl out, OkE E → e' = plug E (.force (.loc l)) →
      Step root lbl Λ' μ' e' out → out = .cfg Λ' μ' (plug E o) := by
  induction hs generalizing Ψ t φ with
  | refl => exact ⟨hd, fun E _ _ hE he hs' => by subst he; exact Step.force_done hE hd hs'⟩
  | step h₁ _ _ ih =>
    obtain ⟨X, hμ', h', hX⟩ := preservation hroot h hμ hb.main hb.mem hΨ h₁
    obtain ⟨hb', hl⟩ := h₁.below hroot.below hb
    exact ih hμ' hb' (fun u hu => by
      rcases List.mem_append.mp hu with hu | hu
      · exact (hΨ u hu).mono hl
      · exact (hX u hu).mono hl) h' (Step.done_stays hμ hμ' h₁ hd)

end Flick
