import Formal.PresHdlRt
import Formal.ScopeRun
import Formal.RaiseTyping

/-!
# The store and the steps of a lazy cell keep their types (lazy.md §3)

A step changes a cell of the store to a content of its type: a value of a `Ref` cell, a state
of a lazy cell (`CellTy`). The steps of a lazy cell keep the type of the term: the initializer
runs in the frame of its cell at the stage of the force, and the outcome it leaves in the cell
is one of the lazy, a value of its type or an error of its row.
-/

namespace Flick

variable {S : Sig}

/-- A raised value of a body with errors `ε₀`, at `Errors ε₀`. -/
theorem raise_inv {Ψ Θ v t φ} (hv : Value v) (h : Typ S Ψ Θ [] (.raise v) (.ex t φ)) :
    ∃ ε, Typ S Ψ Θ [] v (.ex (.errors ε) .pure) ∧ (Stage.rt Row.none ε.asRow).le φ := by
  exact h.raise_value v t φ rfl rfl hv

theorem CellTy.weakenStore {Ψ r ε t C c} (h : CellTy S Ψ r ε t C c) (X : List Ty) :
    CellTy S (Ψ ++ X) r ε t C c := by
  cases h with
  | pending he hψ hn => exact .pending (he.weakenStore X) hψ hn
  | running he hψ hn => exact .running (he.weakenStore X) hψ hn
  | ok hv h => exact .ok hv (h.weakenStore X)
  | err hv h => exact .err hv (h.weakenStore X)
  | defect => exact .defect

/-- What a cell of `Ψ` may hold. -/
def CellOk (S : Sig) (Ψ : List Ty) (c : Tm) (t : Ty) : Prop :=
  (Value c ∧ Typ S Ψ Facts.age [] c (.ex t .pure)) ∨
    ∃ r ε a C, t = .lazyCell r ε a C ∧ CellTy S Ψ r ε a C c

theorem CellOk.weaken {Ψ c t} (h : CellOk S Ψ c t) (X : List Ty) : CellOk S (Ψ ++ X) c t :=
  h.imp (fun ⟨a, b⟩ => ⟨a, b.weakenStore X⟩) fun ⟨r, ε, a, C, e, c⟩ =>
    ⟨r, ε, a, C, e, c.weakenStore X⟩

/-- A new cell. -/
theorem StoreTy.push {Ψ μ c t₀} (hμ : StoreTy S Ψ μ) (h : CellOk S (Ψ ++ [t₀]) c t₀) :
    StoreTy S (Ψ ++ [t₀]) (μ ++ [c]) := by
  refine ⟨by simp [hμ.1], fun i w s hw hs => ?_⟩
  by_cases hi : i < μ.length
  · rw [List.getElem?_append_left hi] at hw
    rw [List.getElem?_append_left (hμ.1 ▸ hi)] at hs
    exact CellOk.weaken (hμ.2 i w s hw hs) _
  · rw [List.getElem?_append_right (by omega)] at hw
    rw [List.getElem?_append_right (by rw [← hμ.1]; omega)] at hs
    rw [hμ.1] at hw
    cases hj : i - Ψ.length with
    | zero => simp [hj] at hw hs; subst hw hs; exact h
    | succ => simp [hj] at hw

theorem StoreTy.alloc {Ψ μ v t₀} (hμ : StoreTy S Ψ μ) (hv : Value v)
    (h : Typ S Ψ Facts.age [] v (.ex t₀ .pure)) :
    StoreTy S (Ψ ++ [t₀]) (μ ++ [v]) :=
  hμ.push (.inl ⟨hv, h.weakenStore _⟩)

theorem StoreTy.allocCell {Ψ μ c r ε t C} (hμ : StoreTy S Ψ μ) (hc : CellTy S Ψ r ε t C c) :
    StoreTy S (Ψ ++ [.lazyCell r ε t C]) (μ ++ [c]) :=
  hμ.push (.inr ⟨_, _, _, _, rfl, hc.weakenStore _⟩)

/-- A cell given a new content of its type. -/
theorem StoreTy.put {Ψ μ l c t₀} (hμ : StoreTy S Ψ μ) (hl : Ψ[l]? = some t₀)
    (h : CellOk S Ψ c t₀) : StoreTy S Ψ (μ.set l c) := by
  refine ⟨by simp [hμ.1], fun i w s hw hs => ?_⟩
  rcases mem_set_get hw with hw | ⟨rfl, rfl⟩
  · exact hμ.2 i w s hw hs
  · rw [hl] at hs; cases hs; exact h

theorem StoreTy.set {Ψ μ l v t₀} (hμ : StoreTy S Ψ μ) (hl : Ψ[l]? = some t₀)
    (_hlt : l < μ.length) (hv : Value v) (h : Typ S Ψ Facts.age [] v (.ex t₀ .pure)) :
    StoreTy S Ψ (μ.set l v) :=
  hμ.put hl (.inl ⟨hv, h⟩)

theorem StoreTy.setCell {Ψ μ l c r ε t C} (hμ : StoreTy S Ψ μ)
    (hl : Ψ[l]? = some (.lazyCell r ε t C)) (hc : CellTy S Ψ r ε t C c) : StoreTy S Ψ (μ.set l c) :=
  hμ.put hl (.inr ⟨_, _, _, _, rfl, hc⟩)

/-- A running cell made pending again keeps its initializer, and with it its type. -/
theorem StoreTy.reset {Ψ μ} (hμ : StoreTy S Ψ μ) (l : Nat) : StoreTy S Ψ (resetCell μ l) := by
  unfold resetCell
  split
  · rename_i e he
    obtain ⟨t₀, ht⟩ : ∃ t₀, Ψ[l]? = some t₀ :=
      ⟨_, List.getElem?_eq_getElem (hμ.1 ▸ getElem?_lt he)⟩
    rcases hμ.2 _ _ _ he ht with ⟨hv, -⟩ | ⟨_, _, _, _, rfl, hc⟩
    · cases hv
    cases hc with
    | running he hψ hn => exact hμ.setCell ht (.pending he hψ hn)
  · exact hμ

theorem StoreTy.settle {Ψ μ l o r ε t C} (hμ : StoreTy S Ψ μ)
    (hl : Ψ[l]? = some (.lazyCell r ε t C)) (hc : CellTy S Ψ r ε t C (.done o)) :
    StoreTy S Ψ (settleCell μ l o) := by
  unfold settleCell
  split
  · exact hμ.setCell hl hc
  · exact hμ

/-- An unchanged store needs no new cells. -/
theorem StoreTy.same {Ψ μ Θ e t φ} (hμ : StoreTy S Ψ μ)
    (h : Typ S Ψ Θ [] e (.ex t φ)) {n : Nat} :
    ∃ X, StoreTy S (Ψ ++ X) μ ∧ Typ S (Ψ ++ X) Θ [] e (.ex t φ) ∧ ∀ u ∈ X, u.YF n :=
  ⟨[], by simpa using hμ, by simpa using h, by simp⟩

/-- A part of what a cell holds, typed where the cell is read: renamed out of the regions not
yet created, under the facts of the place. -/
theorem store_read {Ψ : List Ty} {Θ : Facts} {Λ : List Rgn} {μ : List Tm} {l : Nat} {c x : Tm}
    {J : J} (hcRt : RtCtx Ψ Θ Λ.length)
    (hμb : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Below Λ.length) (hl : μ[l]? = some c)
    (h : Typ S Ψ Facts.age [] x J) (hx : ∀ s, x.Has s → c.Has s) :
    Typ S Ψ Θ [] x (J.substT (kill Λ.length)) := by
  have hk := h.substT (kill Λ.length) (Fits.kill fun p hp => hμb _ _ hl p (hx _ hp))
  have hΨ := List.map_congr_left hcRt.yf
  simp only [List.map_id_fun'] at hΨ
  rw [hΨ] at hk
  exact hk.cut hcRt.inΘ

/-- The type of a lazy cell names no region not yet created. -/
theorem cell_yf {Ψ : List Ty} {Θ : Facts} {n l : Nat} {r ε : Ty} {t : Ty} {C : Caps} (hcRt : RtCtx Ψ Θ n) (hl : Ψ[l]? = some (.lazyCell r ε t C)) :
    r.substT (kill n) = r ∧ ε.substT (kill n) = ε ∧ t.substT (kill n) = t ∧
      C.substT (kill n) = C := by
  have := hcRt.yf _ (List.mem_of_getElem? hl)
  simpa only [Ty.YF, Ty.substT, Ty.lazyCell.injEq] using this

/-- The stage of a force raises the errors of the lazy. -/
theorem Stage.none_err_le {r ε : Row} {C : Caps} {φ : Stage}
    (h : (Stage.use (.rt r ε) C).le φ) : (Stage.rt Row.none ε).le φ := by
  obtain ⟨r', ε', hu⟩ := Stage.use_rt r ε C
  have he : ε' = ε := by simpa [hu, Stage.err] using Stage.use_err (.rt r ε) C
  rw [hu, he] at h
  obtain ⟨_, _, rfl, _, hε⟩ := Stage.rt_le h
  exact ⟨fun _ h => h.elim, hε⟩

/-- `lazy e` makes a cell of the type of the lazy, renamed out of the regions not yet created. -/
theorem lazy_new {Ψ Θ n μ e t₀ ψ r ε C φ} (hμ : StoreTy S Ψ μ) (hcRt : RtCtx Ψ Θ n)
    (he : Typ S Ψ Θ [] e (.ex t₀ ψ)) (hψ : ψ.le (Stage.use (.rt r.asRow ε.asRow) C)) (hn : e.NoIns)
    (ht : (Ty.lazy r ε t₀ C).YFMod n) :
    ∃ X, StoreTy S (Ψ ++ X) (μ ++ [.lazy e]) ∧
      Typ S (Ψ ++ X) Θ [] (.loc μ.length) (.ex (.lazy r ε t₀ C) φ) ∧ ∀ u ∈ X, u.YF n := by
  obtain ⟨r', ε', u, hu, hr, hε, htu⟩ := ht.symm.lazy_left
  simp only [Ty.substT, Ty.lazy.injEq] at hu
  obtain ⟨rfl, rfl, rfl, hC⟩ := hu
  have hψ' : ψ.le (Stage.use (.rt (r.substT (kill n)).asRow (ε.substT (kill n)).asRow) C) := by
    rwa [← hr.asRow, ← hε.asRow]
  refine ⟨[.lazyCell (r.substT (kill n)) (ε.substT (kill n)) (t₀.substT (kill n)) C],
    hμ.allocCell (.pending ((he.conv htu).cut hcRt.outΘ) hψ' hn), ?_, ?_⟩
  · exact (Typ.lazyLoc (by simp [hμ.1])).conv (.lazy hr.symm hε.symm htu.symm)
  · simp only [List.mem_singleton]
    rintro _ rfl
    have hk := Ty.yf_kill n t₀
    have hkr := Ty.yf_kill n r
    have hke := Ty.yf_kill n ε
    simp only [Ty.YF] at hk hkr hke
    simp only [Ty.YF, Ty.substT, hkr, hke, hC, hk]

/-- Forcing a pending cell runs its initializer in the frame of the cell, now running. -/
theorem force_pending {Ψ : List Ty} {Θ : Facts} {Λ : List Rgn} {μ : List Tm} {l : Nat} {e₀ : Tm}
    {r ε : Ty} {t C T φ} (hμ : StoreTy S Ψ μ)
    (hcRt : RtCtx Ψ Θ Λ.length) (hμb : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Below Λ.length)
    (he : Typ S Ψ Θ [] (.loc l) (.ex (.lazy r ε t C) φ))
    (hφ : (Stage.use (.rt r.asRow ε.asRow) C).le φ)
    (hT : LeLive S t T) (hl : μ[l]? = some (.lazy e₀)) {n : Nat} :
    ∃ X, StoreTy S (Ψ ++ X) (μ.set l (.running e₀)) ∧
      Typ S (Ψ ++ X) Θ [] (.settle l e₀) (.ex T φ) ∧ ∀ u ∈ X, u.YF n := by
  obtain ⟨r', ε', u, hl', hr', hε', hu⟩ := he.loc_lazy _ _ _ _ _ _ rfl rfl
  rw [← hr'.asRow, ← hε'.asRow] at hφ
  rcases hμ.2 _ _ _ hl hl' with ⟨hv, -⟩ | ⟨_, _, _, _, heq, hc⟩
  · cases hv
  cases heq
  cases hc with
  | pending he₀ hψ hn =>
    obtain ⟨hr, hε, htk, hC⟩ := cell_yf hcRt hl'
    have hk := store_read hcRt hμb hl he₀ fun _ h => h
    simp only [Flick.J.substT, htk] at hk
    have hψ' := Stage.le_substT (kill Λ.length) hψ
    rw [Stage.use_substT, hC, Stage.substT_rt, Row.substT_asRow, Row.substT_asRow, hr, hε] at hψ'
    exact ⟨[], by simpa using hμ.setCell hl' (.running he₀ hψ hn),
      by simpa using Typ.settle hl' hk hψ' hφ ((LeLive.equiv hu).trans hT), by simp⟩

/-- Forcing a settled cell gives its outcome and leaves the store. -/
theorem force_done {Ψ : List Ty} {Θ : Facts} {Λ : List Rgn} {μ : List Tm} {l : Nat} {o : Tm}
    {r ε : Ty} {t C T φ} (hμ : StoreTy S Ψ μ)
    (hcRt : RtCtx Ψ Θ Λ.length) (hμb : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Below Λ.length)
    (he : Typ S Ψ Θ [] (.loc l) (.ex (.lazy r ε t C) φ))
    (hφ : (Stage.use (.rt r.asRow ε.asRow) C).le φ)
    (hT : LeLive S t T) (hl : μ[l]? = some (.done o)) {n : Nat} :
    ∃ X, StoreTy S (Ψ ++ X) μ ∧ Typ S (Ψ ++ X) Θ [] o (.ex T φ) ∧ ∀ u ∈ X, u.YF n := by
  obtain ⟨r', ε', u, hl', hr', hε', hu⟩ := he.loc_lazy _ _ _ _ _ _ rfl rfl
  rw [← hr'.asRow, ← hε'.asRow] at hφ
  rcases hμ.2 _ _ _ hl hl' with ⟨hv, -⟩ | ⟨_, _, _, _, heq, hc⟩
  · cases hv
  cases heq
  obtain ⟨-, hε, htk, -⟩ := cell_yf hcRt hl'
  cases hc with
  | ok hv h =>
    have hk := store_read hcRt hμb hl h fun _ h => h
    simp only [Flick.J.substT, htk] at hk
    exact hμ.same ((hk.conv hu).sub (J' := .ex _ _) ⟨hT, Stage.pure_le _⟩)
  | err hv h =>
    have hk := store_read hcRt hμb hl h fun _ h => h
    simp only [Flick.J.substT, Ty.substT, hε] at hk
    exact hμ.same (.raise (hk.valueAny hv _) (Stage.none_err_le hφ))
  | defect => exact hμ.same .todo

/-- The initializer's value is the outcome of its cell. -/
theorem settle_val {Ψ Θ n μ l v t C ψ T φ} {r ε : Ty} (hμ : StoreTy S Ψ μ) (hcRt : RtCtx Ψ Θ n)
    (hl : Ψ[l]? = some (.lazyCell r ε t C)) (he : Typ S Ψ Θ [] v (.ex t ψ))
    (hψ : ψ.le (Stage.use (.rt r.asRow ε.asRow) C)) (hφ : (Stage.use (.rt r.asRow ε.asRow) C).le φ)
    (hT : LeLive S t T)
    (hv : Value v) :
    ∃ X, StoreTy S (Ψ ++ X) (settleCell μ l v) ∧ Typ S (Ψ ++ X) Θ [] v (.ex T φ) ∧
      ∀ u ∈ X, u.YF n :=
  ⟨[], by simpa using hμ.settle hl (.ok hv ((he.valueAny hv .pure).cut hcRt.outΘ)),
    by simpa using he.sub (J' := .ex _ _) ⟨hT, Stage.le_trans hψ hφ⟩, by simp⟩

/-- An error the initializer raises is an error of the lazy: its cell keeps it. -/
theorem settle_raise {Ψ Θ n μ l v t C ψ T φ} {r ε : Ty} (hμ : StoreTy S Ψ μ) (hcRt : RtCtx Ψ Θ n)
    (hl : Ψ[l]? = some (.lazyCell r ε t C)) (he : Typ S Ψ Θ [] (.raise v) (.ex t ψ))
    (hψ : ψ.le (Stage.use (.rt r.asRow ε.asRow) C)) (hφ : (Stage.use (.rt r.asRow ε.asRow) C).le φ)
    (hv : Value v) :
    ∃ X, StoreTy S (Ψ ++ X) (settleCell μ l (.raise v)) ∧
      Typ S (Ψ ++ X) Θ [] (.raise v) (.ex T φ) ∧ ∀ u ∈ X, u.YF n := by
  obtain ⟨ε', hv', hε'⟩ := raise_inv hv he
  have hsub : Row.Sub ε'.asRow ε.asRow := fun x hx => by
    simpa using Stage.le_err hψ x (Stage.le_err hε' x hx)
  obtain ⟨_, _, _, _, U, _, _, _, hU, ht, hs⟩ := canonical_errors hv hv'
  exact ⟨[], by simpa using hμ.settle hl (.err hv ((Typ.errVal ht hU (hs.trans hsub)).cut hcRt.outΘ)),
    by simpa using Typ.raise (hv'.valueAny hv φ) (Stage.le_trans hε' (Stage.le_trans hψ hφ)),
    by simp⟩

/-- A signal leaves the frame of a cell, which is pending again. -/
theorem settle_unw {Ψ Θ n μ l p ℓ a t ψ C T φ} {r ε : Ty} (hμ : StoreTy S Ψ μ)
    (he : Typ S Ψ Θ [] (.unw p ℓ a) (.ex t ψ)) (hφ : (Stage.use (.rt r.asRow ε.asRow) C).le φ) :
    ∃ X, StoreTy S (Ψ ++ X) (resetCell μ l) ∧ Typ S (Ψ ++ X) Θ [] (.unw p ℓ a) (.ex T φ) ∧
      ∀ u ∈ X, u.YF n := by
  obtain ⟨_, _, _, _, h₁, h₂, _⟩ := he.unw_type _ _ _ _ _ rfl rfl
  exact (hμ.reset l).same (.unw h₁ h₂ (Stage.none_le (Stage.use_rt_le_ne_pure hφ)))

end Flick
