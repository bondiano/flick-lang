import Formal.Elab.DoNotation

/-!
# Elaboration: `for`, `?` and blocks (elaboration.md §3.2–§3.4)

`GenOk` is the checker's typing of the clauses of `for … yield` and `[for …]`
(`comprehension.rs`): they run at `P` (Sync calls are not modeled), each source `s : f a` with
`Iterable f` gives `Seq a` through `iter`. `LoopOk` types `for … do` at the stage of the
loop. `?` (`question.rs`) is `from_result` at `R` on a `Result`, and a `match` on an `Option`
whose rest returns an `Option`. The statement `s; rest` of a block is `let _ : Unit = s in rest`.
Each desugared form elaborates, and T6(a) covers it.
-/

namespace Flick.Elab

variable {P : Prog} (S : Sugar P)

/-- The clauses of `for … yield e` with elements of type `t`, at `P`. -/
inductive GenOk : List Scheme → List Cn → List Clause → Exp → Ty → Prop
  | ret {Γ Δ e t e'} : Elab P Γ Δ e t .P e' → GenOk Γ Δ [] e t
  | gen {Γ Δ s cs e t f a d s'} : Ent P Δ [⟨S.iterable, [f]⟩] [d] →
      Elab P Γ Δ s (.app f a) .P s' → GenOk (Scheme.mono a :: Γ) Δ cs e t →
      GenOk Γ Δ (.gen s :: cs) e t
  | guard {Γ Δ c cs e t c'} : Elab P Γ Δ c boolTy .P c' → GenOk Γ Δ cs e t →
      GenOk Γ Δ (.guard c :: cs) e t
  | let_ {Γ Δ x cs e t a x'} : Elab P Γ Δ x a .P x' → GenOk (Scheme.mono a :: Γ) Δ cs e t →
      GenOk Γ Δ (.let_ x :: cs) e t

/-- The clauses of `for … do e`, at the stage `φ` of the loop. -/
inductive LoopOk (φ : Stage) : List Scheme → List Cn → List Clause → Exp → Prop
  | ret {Γ Δ e e'} : Elab P Γ Δ e (.con S.unit) φ e' → LoopOk φ Γ Δ [] e
  | gen {Γ Δ s cs e f a d s'} : Ent P Δ [⟨S.iterable, [f]⟩] [d] →
      Elab P Γ Δ s (.app f a) φ s' → LoopOk φ (Scheme.mono a :: Γ) Δ cs e →
      LoopOk φ Γ Δ (.gen s :: cs) e
  | guard {Γ Δ c cs e c'} : Elab P Γ Δ c boolTy φ c' → LoopOk φ Γ Δ cs e →
      LoopOk φ Γ Δ (.guard c :: cs) e
  | let_ {Γ Δ x cs e a x'} : Elab P Γ Δ x a φ x' → LoopOk φ (Scheme.mono a :: Γ) Δ cs e →
      LoopOk φ Γ Δ (.let_ x :: cs) e

/-- `d.iter s : Seq a` -/
theorem elab_iter {Γ Δ s f a φ d s'} (hd : Ent P Δ [⟨S.iterable, [f]⟩] [d])
    (hs : Elab P Γ Δ s (.app f a) φ s') :
    ∃ e', Elab P Γ Δ (.app (.meth S.iter) s) (.app (.con S.seq) a) φ e' := by
  have hi := elab_meth (Γ := Γ) (φ := φ) S.hIter rfl (ts := [f, a]) rfl
    (by simpa [Cn.subst, Ty.subst, inst] using hd)
    (τ := arrP (.app f a) (.app (.con S.seq) a)) rfl
  exact ⟨_, .app hi hs (.inl rfl)⟩

theorem gen_elab {Γ Δ cs e t} (h : GenOk S Γ Δ cs e t) :
    ∃ e', Elab P Γ Δ (S.gen cs e) (.app (.con S.seq) t) .P e' := by
  induction h with
  | @ret Γ Δ e t _ he =>
    exact ⟨_, .app (elab_const S.hSingleton (ts := [t]) rfl (τ := arrP t (.app (.con S.seq) t)) rfl)
      he (.inl rfl)⟩
  | @gen Γ Δ s cs e t f a d s' hd hs _ ih =>
    obtain ⟨_, hk⟩ := ih
    obtain ⟨_, hi⟩ := elab_iter S hd hs
    have hf := elab_const (Γ := Γ) (Δ := Δ) (φ := .P) S.hSeqFlatMap (ts := [a, t]) rfl
      (τ := arrP (arrP a (.app (.con S.seq) t)) (arrP (.app (.con S.seq) a) (.app (.con S.seq) t))) rfl
    exact ⟨_, .app (.app hf (.lam hk) (.inl rfl)) hi (.inl rfl)⟩
  | @guard Γ Δ c cs e t c' hc _ ih =>
    obtain ⟨_, hk⟩ := ih
    exact ⟨_, .ite hc hk (elab_const S.hEmpty (ts := [t]) rfl rfl)⟩
  | let_ hx _ ih => obtain ⟨_, hk⟩ := ih; exact ⟨_, .let_ hx hk⟩

/-- **`for … yield` elaborates**, at any stage: its clauses run when the sequence is used. -/
theorem forSeq_elab {Γ Δ cs e t} (h : GenOk S Γ Δ cs e t) (φ : Stage) :
    ∃ e', Elab P Γ Δ (S.gen cs e) (.app (.con S.seq) t) φ e' :=
  let ⟨_, he⟩ := gen_elab S h
  ⟨_, he.anyStage rfl φ⟩

/-- **`[for … yield e]` elaborates.** -/
theorem forList_elab {Γ Δ cs e t} (h : GenOk S Γ Δ cs e t) (φ : Stage) :
    ∃ e', Elab P Γ Δ (S.forList cs e) (.app (.con S.list) t) φ e' := by
  obtain ⟨_, he⟩ := forSeq_elab S h φ
  exact ⟨_, .app (elab_const S.hToList (ts := [t]) rfl
    (τ := arrP (.app (.con S.seq) t) (.app (.con S.list) t)) rfl) he (.inl rfl)⟩

/-- **`for … do e` elaborates.** -/
theorem loop_elab {φ Γ Δ cs e} (h : LoopOk S φ Γ Δ cs e) :
    ∃ e', Elab P Γ Δ (S.loop φ cs e) (.con S.unit) φ e' := by
  induction h with
  | ret he => exact ⟨_, he⟩
  | @gen Γ Δ s cs e f a d s' hd hs _ ih =>
    obtain ⟨_, hk⟩ := ih
    obtain ⟨_, hi⟩ := elab_iter S hd hs
    have hf := elab_const (Γ := Γ) (Δ := Δ) (φ := φ) (S.hEach φ) (ts := [a]) rfl
      (τ := arrP (arrTy φ a (.con S.unit)) (arrTy φ (.app (.con S.seq) a) (.con S.unit))) rfl
    exact ⟨_, .app (.app hf (.lam hk) (.inl rfl)) hi (.inr rfl)⟩
  | guard hc _ ih =>
    obtain ⟨_, hk⟩ := ih
    exact ⟨_, .ite hc hk (elab_const S.hUnitV (ts := []) rfl rfl)⟩
  | let_ hx _ ih => obtain ⟨_, hk⟩ := ih; exact ⟨_, .let_ hx hk⟩

/-- **`e?` on a `Result` elaborates**, at a runtime stage. -/
theorem tryResult_elab {Γ Δ e ε a e'}
    (he : Elab P Γ Δ e (.app (.app (.con S.result) (.app (.con S.errors) ε)) a) .R e') :
    ∃ e'', Elab P Γ Δ (S.tryResult e) a .R e'' :=
  ⟨_, .app (elab_const S.hFromResult (ts := [ε, a]) rfl
    (τ := arrTy .R (.app (.app (.con S.result) (.app (.con S.errors) ε)) a) a) rfl) he (.inr rfl)⟩

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
  exact ⟨_, .letSig (σ := ⟨0, [], .con S.unit⟩) hs' (by simp) (.inr rfl) (hr.shift _)⟩

/-- **T6(a) for `for`, `?` and blocks**: each elaboration above is typed in the target. -/
theorem T6a_sugar (X : Target P) {Γ Δ e τ φ} (h : ∃ e', Elab P Γ Δ e τ φ e') :
    ∃ e', Elab P Γ Δ e τ φ e' ∧ X.Has (Γ.map Scheme.tr) Δ e' (.mono τ) φ :=
  let ⟨_, he⟩ := h
  ⟨_, he, T6a X he⟩

end Flick.Elab
