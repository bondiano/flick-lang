import Formal.HotSwap.Transfer

/-!
# Typed state-cell migrations and atomic rollback (Theorem 2)
-/

namespace Flick.HotSwap

/-! ## 4. Typed migrations and atomicity -/

/-- A migration request for one declared state cell:
store location `loc` currently holding `oldTy`, target type `newTy`, and pure
migration function `fn : oldTy -> newTy`. -/
structure CellMigration where
  loc : Nat
  oldTy : Ty
  newTy : Ty
  fn : Tm

/-- Static typing requirement for a single cell migration against `(S, Ψ)`. -/
structure CellMigrationOk (S : Sig) (Ψ : List Ty) (m : CellMigration) : Prop where
  cellTy : Ψ[m.loc]? = some m.oldTy
  notLazy : ∀ r ε a C, m.oldTy ≠ .lazyCell r ε a C
  newYF : m.newTy.YF 0
  fnBelow : m.fn.Below 0
  fnNoIns : m.fn.NoIns
  fnTyp : Typ S [] Facts.none [] m.fn (.ex (.arrow m.oldTy .pure m.newTy Caps.none) .pure)

/-- Successful pure evaluation of one cell migration `m` on `(root, Λ, μ)`,
yielding the migrated value `v_new`. -/
def CellMigrationRuns (root : Roots) (Λ : List Rgn) (μ : List Tm)
    (m : CellMigration) (v_new : Tm) : Prop :=
  ∃ v_old, μ[m.loc]? = some v_old ∧ Value v_new ∧
    Steps root (fun _ => True) Λ μ (.app m.fn v_old) Λ μ v_new

/-- Outcome of an atomic batch migration over declared state cells. -/
inductive MigrateOutcome
  | committed
  | rolledBack
  deriving DecidableEq

/-- Atomic batch migration relation on a configuration `c`:
- `commit`: every cell migration `m ∈ ms` evaluates purely to a value `v_new`,
  and the new values `vs` are appended to `c.mem` (keeping all old cells intact
  at their original indices for suspended frames);
- `rollback`: a failing or diverging migration leaves the entire configuration
  `c` completely unchanged. -/
inductive MigrateStep (root : Roots) (ms : List CellMigration) (c : Cfg) :
    MigrateOutcome → Cfg → Prop
  | commit (vs : List Tm) :
      vs.length = ms.length →
      (∀ (i : Nat) m v, ms[i]? = some m → vs[i]? = some v → CellMigrationRuns root c.rgs c.mem m v) →
      MigrateStep root ms c .committed ⟨c.rgs, c.mem ++ vs, c.main⟩
  | rollback :
      MigrateStep root ms c .rolledBack c

/-- A single well-typed cell migration produces a well-typed value at `m.newTy`
that is below `Λ.length` and `NoIns`. -/
theorem cellMigration_result_typed {S : Sig} {root Ψ Λ μ m v_new}
    (hroot : RootTy S root) (hμ : StoreTy S Ψ μ)
    (hμb : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Below Λ.length)
    (hμa : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.NoIns)
    (hΨ : ∀ u ∈ Ψ, u.YF Λ.length)
    (hm : CellMigrationOk S Ψ m) (hrun : CellMigrationRuns root Λ μ m v_new) :
    Value v_new ∧ Typ S Ψ Facts.age [] v_new (.ex m.newTy .pure) ∧
      v_new.Below Λ.length ∧ v_new.NoIns := by
  obtain ⟨v_old, hget, hval, hsteps⟩ := hrun
  rcases hμ.2 m.loc v_old m.oldTy hget hm.cellTy with ⟨_, hvoldTyp⟩ | ⟨r, ε, a, C, hEq, _⟩
  · have hfn : Typ S Ψ Facts.age [] m.fn (.ex (.arrow m.oldTy .pure m.newTy Caps.none) .pure) := by
      have h1 := (hm.fnTyp.cut (Entails.none Facts.age)).weakenStore Ψ
      simpa using h1
    have happ : Typ S Ψ Facts.age [] (.app m.fn v_old) (.ex m.newTy .pure) :=
      .app hfn hvoldTyp (Stage.le_refl _) .refl
    have hblApp : (Tm.app m.fn v_old).Below Λ.length := by
      intro p hp
      rcases hp with h1 | h2
      · exact Nat.lt_of_lt_of_le (hm.fnBelow p h1) (Nat.zero_le _)
      · exact hμb m.loc v_old hget p h2
    have hnoInsOld : v_old.NoIns := hμa m.loc v_old hget
    have hneApp : (Tm.app m.fn v_old).NoIns := fun h => h.elim hm.fnNoIns hnoInsOld
    have hsub : ∀ {Λ₀ μ₀ e Λ' μ' e'},
        Λ₀ = Λ → μ₀ = μ →
        Typ S Ψ Facts.age [] e (.ex m.newTy .pure) →
        e.Below Λ.length → e.NoIns →
        Steps root (fun _ => True) Λ₀ μ₀ e Λ' μ' e' →
        Typ S Ψ Facts.age [] e' (.ex m.newTy .pure) ∧ e'.Below Λ.length ∧ e'.NoIns := by
      intro Λ₀ μ₀ e Λ' μ' e' hΛ₀ hμ₀ he hbe hne hs
      induction hs with
      | refl => exact ⟨he, hbe, hne⟩
      | step h₁ _ _ ih =>
        subst hΛ₀; subst hμ₀
        obtain ⟨hl, hμEq, hΛEq⟩ := step_pure he h₁
        subst hμEq; subst hΛEq; subst hl
        cases h₁ with
        | pure hE hh =>
          obtain ⟨_, _, _, _, hr, _, _, back, -⟩ := ctx_typing _ hE hh.not_value he
          have he' := refill_kill (X := []) back hbe hΨ (by
            simpa using pure_preservation (hole_kill hr hbe hΨ) hh)
          rw [hm.newYF.mono (Nat.zero_le _)] at he'
          have hbe' : (plug _ _).Below _ := fun p hp =>
            (plug_has_inv _ hp).elim (hbe p) (fun h => hbe p (plug_has _ (hh.has h)))
          have hne' : (plug _ _).NoIns := fun h =>
            hne ((plug_has_inv _ h).elim id (fun h' => plug_has _ (hh.has h')))
          exact ih rfl rfl (by simpa [Stage.substT] using he') hbe' hne'
        | glob hE hg =>
          obtain ⟨_, _, _, _, hr, _, _, back, -⟩ := ctx_typing _ hE (fun h => by cases h) he
          have he' := refill_kill (X := []) back hbe hΨ (by
            simpa using glob_preservation hroot (hole_kill hr hbe hΨ) hg)
          rw [hm.newYF.mono (Nat.zero_le _)] at he'
          obtain ⟨hno, _, _, _⟩ := hroot.glob _ _ hg
          have hbe' : (plug _ _).Below _ := fun p hp =>
            (plug_has_inv _ hp).elim (hbe p) (fun h => (hno _ h).elim)
          have hne' : (plug _ _).NoIns := fun h =>
            hne ((plug_has_inv _ h).elim id (fun h' => (hno _ h').elim))
          exact ih rfl rfl (by simpa [Stage.substT] using he') hbe' hne'
    obtain ⟨h1, h2, h3⟩ := hsub rfl rfl happ hblApp hneApp hsteps
    exact ⟨hval, h1, h2, h3⟩
  · exact (hm.notLazy r ε a C hEq).elim

/-- Helper: extending `StoreTy` with a list of typed pure values. -/
theorem storeTy_append_values {S : Sig} {Ψ μ vs X}
    (hμ : StoreTy S Ψ μ) (hlen : vs.length = X.length)
    (hvs : ∀ (i : Nat) v u, vs[i]? = some v → X[i]? = some u →
      Value v ∧ Typ S Ψ Facts.age [] v (.ex u .pure)) :
    StoreTy S (Ψ ++ X) (μ ++ vs) := by
  refine ⟨by simp [hμ.1, hlen], fun i v u hv hu => ?_⟩
  by_cases hi : i < μ.length
  · have hi' : i < Ψ.length := hμ.1 ▸ hi
    rw [List.getElem?_append_left hi] at hv
    rw [List.getElem?_append_left hi'] at hu
    rcases hμ.2 i v u hv hu with ⟨hval, htyp⟩ | ⟨r, ε, a, C, hEq, hcell⟩
    · exact .inl ⟨hval, htyp.weakenStore X⟩
    · exact .inr ⟨r, ε, a, C, hEq, hcell.weakenStore X⟩
  · have hi' : ¬ i < Ψ.length := fun h => hi (hμ.1.symm ▸ h)
    rw [List.getElem?_append_right (Nat.le_of_not_lt hi)] at hv
    rw [List.getElem?_append_right (Nat.le_of_not_lt hi')] at hu
    obtain ⟨hval, htyp⟩ := hvs (i - Ψ.length) v u (by rwa [← hμ.1]) hu
    exact .inl ⟨hval, htyp.weakenStore X⟩

/-- **Theorem 2a (Preservation under a well-typed migration).**
Committing a well-typed batch of pure state-cell migrations extends the store
typing `Ψ` to `Ψ ++ X` (`X = ms.map (·.newTy)`), preserves `CfgTy` on the
updated configuration `c'`, keeps every pre-migration cell intact at its old
type (so suspended frames continue to read old versions safely), and types
every migrated cell at its new type. -/
theorem migration_preserves_cfgTy {S : Sig} {root Ψ c t φ ms c'}
    (hroot : RootTy S root) (hc : CfgTy S Ψ c t φ)
    (hms : ∀ m ∈ ms, CellMigrationOk S Ψ m)
    (hstep : MigrateStep root ms c .committed c') :
    let X := ms.map (·.newTy)
    CfgTy S (Ψ ++ X) c' t φ ∧
      (∀ u ∈ X, u.YF c'.rgs.length) ∧
      (∀ (i : Nat), i < c.mem.length → c'.mem[i]? = c.mem[i]? ∧ (Ψ ++ X)[i]? = Ψ[i]?) ∧
      (∀ (k : Nat) m, ms[k]? = some m →
        ∃ v, c'.mem[c.mem.length + k]? = some v ∧
          (Ψ ++ X)[c.mem.length + k]? = some m.newTy ∧
          Value v ∧ Typ S (Ψ ++ X) Facts.age [] v (.ex m.newTy .pure)) := by
  cases hstep with
  | commit vs hvlen hruns =>
    obtain ⟨hμ, hm, hΛ, hb, hΨ, ha, hs, cell, hcg⟩ := hc
    let X := ms.map (·.newTy)
    have hXlen : vs.length = X.length := by simp [X, hvlen]
    have hcellRes : ∀ (k : Nat) m v, ms[k]? = some m → vs[k]? = some v →
        Value v ∧ Typ S Ψ Facts.age [] v (.ex m.newTy .pure) ∧
          v.Below c.rgs.length ∧ v.NoIns := by
      intro k m v hmk hvk
      exact cellMigration_result_typed hroot hμ hb.mem ha.mem hΨ
        (hms m (List.mem_of_getElem? hmk)) (hruns k m v hmk hvk)
    have hμ' : StoreTy S (Ψ ++ X) (c.mem ++ vs) := by
      refine storeTy_append_values hμ hXlen fun k v u hvk huk => ?_
      simp only [X, List.getElem?_map, Option.map_eq_some_iff] at huk
      obtain ⟨m, hmk, rfl⟩ := huk
      obtain ⟨hval, htyp, _, _⟩ := hcellRes k m v hmk hvk
      exact ⟨hval, htyp⟩
    have hb' : CfgBelow c.rgs (c.mem ++ vs) c.main := by
      refine ⟨hb.main, fun l v hv => ?_, hb.rgs⟩
      by_cases hl : l < c.mem.length
      · rw [List.getElem?_append_left hl] at hv; exact hb.mem l v hv
      · rw [List.getElem?_append_right (Nat.le_of_not_lt hl)] at hv
        have hk : l - c.mem.length < ms.length := by
          have := getElem?_lt hv; omega
        obtain ⟨m, hmk⟩ : ∃ m, ms[l - c.mem.length]? = some m :=
          ⟨_, List.getElem?_eq_getElem hk⟩
        exact (hcellRes _ m v hmk hv).2.2.1
    have ha' : CfgAsc c.rgs (c.mem ++ vs) c.main := by
      refine ⟨ha.main, ha.rgs, fun l v hv => ?_⟩
      by_cases hl : l < c.mem.length
      · rw [List.getElem?_append_left hl] at hv; exact ha.mem l v hv
      · rw [List.getElem?_append_right (Nat.le_of_not_lt hl)] at hv
        have hk : l - c.mem.length < ms.length := by
          have := getElem?_lt hv; omega
        obtain ⟨m, hmk⟩ : ∃ m, ms[l - c.mem.length]? = some m :=
          ⟨_, List.getElem?_eq_getElem hk⟩
        exact (hcellRes _ m v hmk hv).2.2.2
    have hXyf : ∀ u ∈ X, u.YF c.rgs.length := by
      intro u hu
      simp only [X, List.mem_map] at hu
      obtain ⟨m, hmem, rfl⟩ := hu
      exact (hms m hmem).newYF.mono (Nat.zero_le _)
    have hΨ' : ∀ u ∈ Ψ ++ X, u.YF c.rgs.length := fun u hu =>
      (List.mem_append.mp hu).elim (hΨ u) (hXyf u)
    refine ⟨⟨hμ', hm.weakenStore X, hΛ.weaken X, hb', hΨ', ha', hs, cell, hcg⟩, hXyf, ?_, ?_⟩
    · intro i hi
      exact ⟨List.getElem?_append_left hi, List.getElem?_append_left (hμ.1 ▸ hi)⟩
    · intro k m hmk
      have hk : k < vs.length := hvlen.symm ▸ getElem?_lt hmk
      obtain ⟨v, hvk⟩ : ∃ v, vs[k]? = some v := ⟨_, List.getElem?_eq_getElem hk⟩
      obtain ⟨hval, htyp, _, _⟩ := hcellRes k m v hmk hvk
      refine ⟨v, ?_, ?_, hval, htyp.weakenStore X⟩
      · rw [List.getElem?_append_right (Nat.le_add_right _ _)]
        simpa using hvk
      · rw [List.getElem?_append_right (by rw [← hμ.1]; exact Nat.le_add_right _ _)]
        simp [hμ.1, hmk]

/-- **Theorem 2b (Migration atomicity on failure or divergence).**
When a migration fails or diverges, the entire configuration is unchanged and
remains well typed at the original store typing `Ψ`. -/
theorem migration_rollback_atomic {S : Sig} {root Ψ c t φ ms c'}
    (hc : CfgTy S Ψ c t φ) (hstep : MigrateStep root ms c .rolledBack c') :
    c' = c ∧ CfgTy S Ψ c' t φ := by
  cases hstep
  exact ⟨rfl, hc⟩

/-- Same-type in-place state-cell migration (`m.newTy = m.oldTy`):
updating `c.mem[m.loc]` in place preserves `CfgTy` at the exact same store
typing `Ψ`. -/
theorem migration_in_place_preserves_cfgTy {S : Sig} {root Ψ c t φ m v_new}
    (hroot : RootTy S root) (hc : CfgTy S Ψ c t φ)
    (hm : CellMigrationOk S Ψ m) (hSame : m.newTy = m.oldTy)
    (hrun : CellMigrationRuns root c.rgs c.mem m v_new) :
    CfgTy S Ψ ⟨c.rgs, c.mem.set m.loc v_new, c.main⟩ t φ := by
  obtain ⟨hμ, hmMain, hΛ, hb, hΨ, ha, hs, cell, hcg⟩ := hc
  obtain ⟨hval, htyp, hblNew, hnoInsNew⟩ :=
    cellMigration_result_typed hroot hμ hb.mem ha.mem hΨ hm hrun
  rw [hSame] at htyp
  have hμ' : StoreTy S Ψ (c.mem.set m.loc v_new) := by
    refine ⟨by simp [hμ.1], fun i v u hv hu => ?_⟩
    rcases mem_set_get hv with hv' | ⟨rfl, rfl⟩
    · exact hμ.2 i v u hv' hu
    · rw [hm.cellTy] at hu; cases hu
      exact .inl ⟨hval, htyp⟩
  have hb' : CfgBelow c.rgs (c.mem.set m.loc v_new) c.main := by
    refine ⟨hb.main, fun l v hv => ?_, hb.rgs⟩
    rcases mem_set_get hv with hv' | ⟨rfl, rfl⟩
    · exact hb.mem l v hv'
    · exact hblNew
  have ha' : CfgAsc c.rgs (c.mem.set m.loc v_new) c.main := by
    refine ⟨ha.main, ha.rgs, fun l v hv => ?_⟩
    rcases mem_set_get hv with hv' | ⟨rfl, rfl⟩
    · exact ha.mem l v hv'
    · exact hnoInsNew
  exact ⟨hμ', hmMain, hΛ, hb', hΨ, ha', hs, cell, hcg⟩


end Flick.HotSwap
