import Formal.Elab.DoNotation

/-!
# Elaboration: `for`, `?` and blocks (elaboration.md §3.2–§3.4)

`GenOk` is the checker's typing of the clauses of `for … yield` and `[for …]`
(`comprehension.rs`): they run at `P` (Sync calls are not modeled), each source `s : f d a` with
`Iterable f` gives `Seq (d ∪ a) a` through `iter`. `LoopOk` types `for … do` at the stage of the
loop. `?` (`question.rs`) is `from_result` at `R` on a `Result`, and a `match` on an `Option`
whose rest returns an `Option`. The statement `s; rest` of a block is `let _ : Unit = s in rest`.
Each desugared form elaborates, and T6(a) covers it.
-/

namespace Flick.Elab

variable {P : Prog} (S : Sugar P)

/-- The clauses of `for … yield e` with elements of type `t`, at `P`. -/
inductive GenOk : List Scheme → List Cn → List Clause → Exp → Ty → Prop
  | ret {Γ Δ e t e'} : Elab P Γ Δ e t .P e' → GenOk Γ Δ [] e t
  | gen {Γ Δ s cs e t f d a dk s'} : Ent P Δ [⟨S.iterable, [f]⟩] [dk] →
      Elab P Γ Δ s (appQ f d a) .P s' → GenOk (Scheme.mono a :: Γ) Δ cs e t →
      GenOk Γ Δ (.gen s :: cs) e t
  | guard {Γ Δ c cs e t c'} : Elab P Γ Δ c boolTy .P c' → GenOk Γ Δ cs e t →
      GenOk Γ Δ (.guard c :: cs) e t
  | let_ {Γ Δ x cs e t a x'} : Elab P Γ Δ x a .P x' → GenOk (Scheme.mono a :: Γ) Δ cs e t →
      GenOk Γ Δ (.let_ x :: cs) e t

/-- The clauses of `for … do e`, at the stage `φ` of the loop. -/
inductive LoopOk (φ : Stage) : List Scheme → List Cn → List Clause → Exp → Prop
  | ret {Γ Δ e e'} : Elab P Γ Δ e (.con S.unit) φ e' → LoopOk φ Γ Δ [] e
  | gen {Γ Δ s cs e f d a dk s'} : Ent P Δ [⟨S.iterable, [f]⟩] [dk] →
      Elab P Γ Δ s (appQ f d a) φ s' → LoopOk φ (Scheme.mono a :: Γ) Δ cs e →
      LoopOk φ Γ Δ (.gen s :: cs) e
  | guard {Γ Δ c cs e c'} : Elab P Γ Δ c boolTy φ c' → LoopOk φ Γ Δ cs e →
      LoopOk φ Γ Δ (.guard c :: cs) e
  | let_ {Γ Δ x cs e a x'} : Elab P Γ Δ x a φ x' → LoopOk φ (Scheme.mono a :: Γ) Δ cs e →
      LoopOk φ Γ Δ (.let_ x :: cs) e

/-- `d.iter s : Seq (d ∪ a) a` -/
theorem elab_iter {Γ Δ s f d a φ dk s'} (hd : Ent P Δ [⟨S.iterable, [f]⟩] [dk])
    (hs : Elab P Γ Δ s (appQ f d a) φ s') :
    ∃ e', Elab P Γ Δ (.app (.meth S.iter) s) (appQ (.con S.seq) (.cun d a) a) φ e' := by
  have hi := elab_meth (Γ := Γ) (φ := φ) S.hIter rfl (ts := [f, d, a]) rfl
    (by simpa [Cn.subst, Ty.subst, inst] using hd)
    (τ := arrP (appQ f d a) (appQ (.con S.seq) (.cun d a) a)) rfl
  exact ⟨_, .app hi hs (.inl rfl)⟩

theorem gen_elab {Γ Δ cs e t} (h : GenOk S Γ Δ cs e t) :
    ∃ q e', Elab P Γ Δ (S.gen cs e) (appQ (.con S.seq) q t) .P e' := by
  induction h with
  | @ret Γ Δ e t _ he =>
    exact ⟨.cnil, _, .app (elab_const S.hSingleton (ts := [t, .cnil]) rfl
      (τ := arrP t (appQ (.con S.seq) .cnil t)) rfl) he (.inl rfl)⟩
  | @gen Γ Δ s cs e t f d a dk s' hd hs _ ih =>
    obtain ⟨q, _, hk⟩ := ih
    obtain ⟨_, hi⟩ := elab_iter S hd hs
    have hf := elab_const (Γ := Γ) (Δ := Δ) (φ := .P) S.hSeqFlatMap
      (ts := [ctxRow Γ, .cun d a, a, t, q]) rfl (τ := bindTy (.con S.seq) (ctxRow Γ) (.cun d a) a t q) rfl
    exact ⟨_, _, .app (.app hf (.lam hk (lam_cov (cov_ctxRow Γ))) (.inl rfl)) hi (.inl rfl)⟩
  | @guard Γ Δ c cs e t c' hc _ ih =>
    obtain ⟨q, _, hk⟩ := ih
    exact ⟨_, _, .ite hc hk (elab_const S.hEmpty (ts := [t, q]) rfl rfl)⟩
  | let_ hx _ ih => obtain ⟨_, _, hk⟩ := ih; exact ⟨_, _, .let_ hx hk⟩

/-- **`for … yield` elaborates**, at any stage: its clauses run when the sequence is used. -/
theorem forSeq_elab {Γ Δ cs e t} (h : GenOk S Γ Δ cs e t) (φ : Stage) :
    ∃ q e', Elab P Γ Δ (S.gen cs e) (appQ (.con S.seq) q t) φ e' :=
  let ⟨_, _, he⟩ := gen_elab S h
  ⟨_, _, he.anyStage rfl φ⟩

/-- **`[for … yield e]` elaborates.** -/
theorem forList_elab {Γ Δ cs e t} (h : GenOk S Γ Δ cs e t) (φ : Stage) :
    ∃ e', Elab P Γ Δ (S.forList cs e) (.app (.con S.list) t) φ e' := by
  obtain ⟨q, _, he⟩ := forSeq_elab S h φ
  exact ⟨_, .app (elab_const S.hToList (ts := [q, t]) rfl
    (τ := arrP (appQ (.con S.seq) q t) (.app (.con S.list) t)) rfl) he (.inl rfl)⟩

/-- **`for … do e` elaborates.** -/
theorem loop_elab {φ Γ Δ cs e} (h : LoopOk S φ Γ Δ cs e) :
    ∃ e', Elab P Γ Δ (S.loop φ cs e) (.con S.unit) φ e' := by
  induction h with
  | ret he => exact ⟨_, he⟩
  | @gen Γ Δ s cs e f d a dk s' hd hs _ ih =>
    obtain ⟨_, hk⟩ := ih
    obtain ⟨_, hi⟩ := elab_iter S hd hs
    have hf : Elab P Γ Δ (.const (S.each φ)) (arrP (.arr φ a (.con S.unit) (ctxRow Γ))
        (.arr φ (appQ (.con S.seq) (.cun d a) a) (.con S.unit) (ctxRow Γ))) φ (.const (S.each φ)) := by
      cases φ with
      | P => exact elab_const (S.hEach .P) (ts := [ctxRow Γ, .cun d a, a]) rfl rfl
      | R r e => exact elab_const (S.hEach (.R r e)) (ts := [ctxRow Γ, .cun d a, a, r, e]) rfl rfl
    exact ⟨_, .app (.app hf (.lam hk (lam_cov (cov_ctxRow Γ))) (.inl rfl)) hi (.inr rfl)⟩
  | guard hc _ ih =>
    obtain ⟨_, hk⟩ := ih
    exact ⟨_, .ite hc hk (elab_const S.hUnitV (ts := []) rfl rfl)⟩
  | let_ hx _ ih => obtain ⟨_, hk⟩ := ih; exact ⟨_, .let_ hx hk⟩

/-- **`e?` on a `Result` elaborates**, at the runtime stage `R⟨∅; con n⟩`, through the member
of `from_result` at the error type `con n` of `e`. -/
theorem tryResult_elab {Γ Δ e n a e'}
    (he : Elab P Γ Δ e (.app (.app (.con S.result) (.app (.con S.errors) (.con n))) a)
      (.R .cnil (.con n)) e') :
    ∃ e'', Elab P Γ Δ (S.tryResult n e) a (.R .cnil (.con n)) e'' :=
  ⟨_, .app (elab_const (S.hFromResult n) (ts := [a]) rfl
    (τ := .arr (.R .cnil (.con n)) (.app (.app (.con S.result) (.app (.con S.errors) (.con n))) a) a
      .cnil) rfl) he (.inr rfl)⟩

/-- **`let y = e? in b` on an `Option` elaborates** when the rest returns an `Option`. -/
theorem tryOption_elab {Γ Δ e b a r φ e' b'} (he : Elab P Γ Δ e (optTy a) φ e')
    (hb : Elab P (Scheme.mono a :: Γ) Δ b (optTy r) φ b') :
    ∃ e'', Elab P Γ Δ (S.tryOption e b) (optTy r) φ e'' :=
  ⟨_, .caseOpt he (elab_const S.hNone (ts := [r]) rfl rfl) hb⟩

theorem Scheme.subst_up_zero (s : Scheme) : s.subst (up 0) = s := by
  simp [Scheme.subst, lift_up_zero, Ty.subst_id, Cn.subst, List.map_id']

/-- **`s; rest` elaborates.** -/
theorem seqUnit_elab {Γ Δ s rest τ φ s' r'} (hs : Elab P Γ Δ s (.con S.unit) φ s')
    (hr : Elab P Γ Δ rest τ φ r') : ∃ e', Elab P Γ Δ (S.seqUnit s rest) τ φ e' := by
  have hs' : Elab P (Γ.map (·.subst (up 0))) ([] ++ Δ.map (·.subst (up 0))) s (.con S.unit) φ s' := by
    simpa [Scheme.subst_up_zero, Cn.subst_up_zero] using hs
  exact ⟨_, .letSig (σ := ⟨0, [], .con S.unit, .cnil⟩) hs' (by simp) (.inr rfl)
    (fun h => absurd rfl h) (hr.shift _)⟩

/-- **T6(a) for `for`, `?` and blocks**: each elaboration above is typed in the target. -/
theorem T6a_sugar (X : Target P) {Γ Δ e τ φ} (h : ∃ e', Elab P Γ Δ e τ φ e') :
    ∃ e', Elab P Γ Δ e τ φ e' ∧ X.Has (Γ.map Scheme.tr) Δ e' (.mono τ) φ :=
  let ⟨_, he⟩ := h
  ⟨_, he, T6a X he⟩

end Flick.Elab
