import Formal.Elab.CoreTyDefs

/-!
# Substitution, reach, and stage-order lemmas for `trT` (`Formal.Elab.CoreTy`)
-/

namespace Flick.Elab

variable (N : Names)

/-- `⌈⟦τ⟧⌉` is what `τ` shows: only variables. -/
theorem trT_reach (t : Ty) (a : Atom) : (trT N t).reach a ↔ ∃ n, a = .v n ∧ t.Shown n := by
  induction t using Ty.ind generalizing a with
  | var m => simp [trT, Ty.Shown]
  | con c =>
    simp only [trT, Ty.Shown, and_false, exists_false, iff_false]
    split <;> (try split) <;> simp [Flick.Ty.reach, Caps.none]
  | app t u iht ihu => simp only [trT, Flick.Ty.reach, iht, ihu, Ty.Shown, ex_or]
  | arr φ a b q iha ihb ihq _ =>
    simp only [trT_arr, Flick.Ty.reach, iha, ihb, ihq, Ty.Shown, ex_or]
  | cnil => simp [trT, Flick.Ty.reach, Caps.none, Ty.Shown]
  | cun q q' ihq ihq' => simp only [trT, Flick.Ty.reach, ihq, ihq', Ty.Shown, ex_or]
  | rnm c t iht => simp [trT, Flick.Ty.reach, iht, Ty.Shown]
  | all k t iht =>
    simp only [trT, Flick.Ty.reach, iht, Ty.Shown]
    cases a <;> simp [Atom.up]
  | run r r' ihr ihr' => simp only [trT, Flick.Ty.reach, ihr, ihr', Ty.Shown, ex_or]

theorem trT_reach_v {t : Ty} {n : Nat} (h : t.Shown n) : (trT N t).reach (.v n) :=
  (trT_reach N t _).2 ⟨n, rfl, h⟩

/-- What `⟦t⟧` reaches are variables of `t`. -/
theorem trT_reach_sub {t : Ty} {a : Atom} (h : (trT N t).reach a) : ∃ n, a = .v n ∧ t.Occurs n :=
  let ⟨n, e, hn⟩ := (trT_reach N t a).1 h; ⟨n, e, hn.occurs⟩

/-- A row that keeps `n` live has its atom: `Scope s`, or a capture row written in it. -/
theorem trT_keeps {r : Ty} {n : Nat} (h : r.Keeps n) : (trT N r).asRow (.atm (.v n)) := by
  induction r using Ty.ind with
  | var m => exact h.elim
  | rnm c t _ => exact .inr (trT_reach_v N h)
  | run r r' ihr ihr' =>
    rcases h with h | h
    · exact .inl (ihr h)
    · exact .inr (ihr' h)
  | con c =>
    simp only [Ty.Keeps, Ty.Shown] at h
  | app t u _ _ => exact trT_reach_v N h
  | arr φ a b q _ _ _ _ =>
    have := trT_reach_v N h
    rw [trT_arr] at this ⊢
    exact this
  | cnil => exact h.elim
  | cun q q' _ _ => exact trT_reach_v N h
  | all k t _ => exact trT_reach_v N h

theorem trD_reach (q : Cn) (a : Atom) : ¬ (trD N q).reach a := by
  simp [trD, Flick.Ty.reach, Caps.none]

theorem trTT_reach (t : TTy) (a : Atom) : (trTT N t).reach a ↔ ∃ n, a = .v n ∧ t.reach n := by
  induction t generalizing a with
  | mono t => exact trT_reach N t a
  | arr φ a b q iha ihb =>
    simp only [trTT_arr, Flick.Ty.reach, iha, ihb, trT_reach, TTy.reach, ex_or]
  | dict q => simp [trTT, trD_reach, TTy.reach]

theorem Caps.substT_or (C C' : Caps) (σ : Atom → Flick.Ty) :
    Caps.substT σ (fun a => C a ∨ C' a) = fun a => C.substT σ a ∨ C'.substT σ a := by
  funext a; apply propext; simp only [Caps.substT]
  constructor
  · rintro ⟨b, h | h, hb⟩
    · exact .inl ⟨b, h, hb⟩
    · exact .inr ⟨b, h, hb⟩
  · rintro (⟨b, h, hb⟩ | ⟨b, h, hb⟩)
    · exact ⟨b, .inl h, hb⟩
    · exact ⟨b, .inr h, hb⟩

/-- The translation commutes with a substitution whose types shift as they should. -/
theorem trT_subst_of (t : Ty) (σ : Nat → Ty)
    (hσ : ∀ k n, trT N ((σ n).ren (· + k)) = (trT N (σ n)).substT (shiftV k)) :
    trT N (t.subst σ) = (trT N t).substT (trσ N σ) := by
  induction t using Ty.ind generalizing σ with
  | all k t iht =>
    have hl : trσ N (liftTy k σ) = liftT k (trσ N σ) := by
      funext a
      cases a with
      | v n =>
        simp only [trσ, liftTy, liftT]
        split
        · rfl
        · rw [hσ, Flick.Ty.up_def]
      | r p => simp [trσ, liftT, Flick.Ty.up_def, Flick.Ty.substT, shiftV]
    simp only [Ty.subst, trT, Flick.Ty.substT]
    rw [iht _ fun j n => ?_, hl]
    simp only [liftTy]
    split
    · simp [Ty.ren, trT, Flick.Ty.substT, shiftV]
    · rw [Ty.ren_ren, show (fun x => x + k + j) = (· + (k + j)) by funext x; omega, hσ, hσ,
        Flick.Ty.substT_comp]
      congr 1; funext a
      cases a with
      | v i => simp [shiftV, Flick.Ty.substT]; omega
      | r p => rfl
  | var n => rfl
  | con c => simp only [Ty.subst, trT]; split <;> (try split) <;> rfl
  | app t u iht ihu => simp [Ty.subst, trT, Flick.Ty.substT, iht σ hσ, ihu σ hσ]
  | arr φ a b q iha ihb ihq ihφ =>
    have hφ : trStageT N (φ.subst σ) = (trStageT N φ).substS (trσ N σ) := by
      cases φ with
      | P => rfl
      | R r e =>
        simp [Stage.subst, trStageT, ihφ.1 σ hσ, ihφ.2 σ hσ,
          errT_substT]
    have hq : (trT N (q.subst σ)).reach = Caps.substT (trσ N σ) (trT N q).reach := by
      rw [ihq σ hσ, ← Ty.reach_caps]
    simp only [Ty.subst, trT_arr, Flick.Ty.substT_arrow, iha σ hσ, ihb σ hσ, hφ, hq]
  | cnil => simp [Ty.subst, trT, Flick.Ty.substT]
  | cun q q' ihq ihq' =>
    simp only [Ty.subst, trT, Flick.Ty.substT, ihq σ hσ, ihq' σ hσ, Caps.substT_or, Ty.reach_caps]
  | rnm c t iht => simp only [Ty.subst, trT, Flick.Ty.substT, iht σ hσ]
  | run r r' ihr ihr' => simp only [Ty.subst, trT, Flick.Ty.substT, ihr σ hσ, ihr' σ hσ]

theorem trT_ren (t : Ty) (k : Nat) : trT N (t.ren (· + k)) = (trT N t).substT (shiftV k) := by
  rw [Ty.ren_eq, trT_subst_of N t _ fun _ _ => rfl]
  congr 1

/-- **The translation commutes with substitution.** -/
theorem trT_subst (t : Ty) (σ : Nat → Ty) : trT N (t.subst σ) = (trT N t).substT (trσ N σ) :=
  trT_subst_of N t σ fun k n => trT_ren N (σ n) k

theorem trStageT_subst (φ : Stage) (σ : Nat → Ty) :
    trStageT N (φ.subst σ) = (trStageT N φ).substS (trσ N σ) := by
  cases φ with
  | P => rfl
  | R r e => simp [Stage.subst, trStageT, trT_subst, errT_substT]

theorem trStage_subst (φ : Stage) (σ : Nat → Ty) :
    trStage N (φ.subst σ) = (trStage N φ).substT (trσ N σ) := by
  simp only [trStage, trStageT_subst, Ty.asStage_substS]

theorem trD_subst (q : Cn) (σ : Nat → Ty) : trD N (q.subst σ) = (trD N q).substT (trσ N σ) := by
  simp only [trD, Cn.subst, Flick.Ty.substT, Flick.Ty.spine_substT, List.map_map, List.length_map]
  congr 2
  exact List.map_congr_left fun t _ => trT_subst N t σ

theorem trTT_subst (t : TTy) (σ : Nat → Ty) :
    trTT N (t.subst σ) = (trTT N t).substT (trσ N σ) := by
  induction t with
  | mono t => exact trT_subst N t σ
  | arr φ a b q iha ihb =>
    have hq : (trT N (q.subst σ)).reach = Caps.substT (trσ N σ) (trT N q).reach := by
      rw [trT_subst, ← Ty.reach_caps]
    simp only [TTy.subst, trTT_arr, Flick.Ty.substT_arrow, iha, ihb, trStageT_subst, hq]
  | dict q => exact trD_subst N q σ

theorem trσ_lift (k : Nat) (σ : Nat → Ty) (a : Atom) : trσ N (lift k σ) a = liftT k (trσ N σ) a := by
  cases a with
  | v n =>
    simp only [trσ, lift, liftT]
    split
    · rfl
    · rw [trT_subst, Flick.Ty.up_def]; apply Flick.Ty.substT_congr; intro b; cases b <;> rfl
  | r p => rfl

theorem trσ_up (k : Nat) (a : Atom) : trσ N (up k) a = shiftV k a := by
  cases a <;> rfl

theorem trσ_inst (ts : List Ty) (a : Atom) : trσ N (inst ts) a = instT (ts.map (trT N)) a := by
  cases a with
  | v n => simp only [trσ, inst, instT, List.length_map]; split <;> simp [trT]
  | r p => rfl

theorem trB_var (i : Nat) (D : Ty) : trB N (.var i, D) = Facts.bnd i (trT N D).reach := rfl

/-- A value bound as a fact under any substitution of the core. -/
theorem trB_substT_any (b : Ty × Ty) (ρ : Atom → Flick.Ty) :
    (trB N b).substT ρ =
      fun C r => C = ((trT N b.1).substT ρ).reach ∧ r = .caps ((trT N b.2).substT ρ).reach := by
  funext C r; apply propext
  simp only [trB, Facts.substT, ← Ty.reach_caps]
  constructor
  · rintro ⟨_, _, ⟨rfl, rfl⟩, rfl, rfl⟩; exact ⟨rfl, rfl⟩
  · rintro ⟨rfl, rfl⟩; exact ⟨_, _, ⟨rfl, rfl⟩, rfl, rfl⟩

theorem trB_substT (b : Ty × Ty) (σ : Nat → Ty) :
    (trB N b).substT (trσ N σ) = trB N (Bound.subst σ b) := by
  rw [trB_substT_any]; unfold trB; simp only [Bound.subst, trT_subst]

theorem trBd_substT (bd : List (Ty × Ty)) (σ : Nat → Ty) :
    (trBd N bd).substT (trσ N σ) = trBd N (bd.map (Bound.subst σ)) := by
  induction bd with
  | nil => exact Facts.none_substT _
  | cons b bd ih => simp only [trBd, List.map_cons, Facts.union_substT, trB_substT, ih]

/-- The facts of value bounds hold under the translation of a substitution that makes the
bounds hold: `⌈⟦t⟧⌉ ⊆ ⌈⟦D⟧⌉` is what `t` shows within what `D` shows (`trT_reach`). -/
theorem trBd_holds {bd : List (Ty × Ty)} {σ : Nat → Ty} {ρ : Atom → Flick.Ty}
    (hρ : ∀ a, ρ a = trσ N σ a) (h : Bounds.Hold (bd.map (Bound.subst σ))) (Θ : Facts) :
    ∀ C r, trBd N bd C r → Outl Θ (C.substT ρ) (r.substT ρ) := by
  rw [funext hρ]
  induction bd with
  | nil => intro _ _ h; exact h.elim
  | cons b bd ih =>
    rintro C r (⟨rfl, rfl⟩ | hr)
    · refine Outl.caps_of_sub fun a ha => ?_
      rw [Ty.reach_caps, ← trT_subst] at ha ⊢
      obtain ⟨n, rfl, hn⟩ := (trT_reach N _ a).1 ha
      exact trT_reach_v N (h _ (.head _) n hn)
    · exact ih (fun b hb => h b (.tail _ hb)) C r hr

theorem trFacts_holds {k : Nat} {bd : List (Ty × Ty)} {σ : Nat → Ty} {ρ : Atom → Flick.Ty}
    (hρ : ∀ a, ρ a = trσ N σ a) (h : Bounds.Hold (bd.map (Bound.subst σ))) (Θ : Facts) :
    ∀ C r, trFacts N k bd C r → Outl Θ (C.substT ρ) (r.substT ρ) := by
  unfold trFacts; split
  · intro _ _ h; exact h.elim
  · exact trBd_holds N hρ h Θ

theorem trS_subst (s : TSch) (σ : Nat → Ty) : trS N (s.subst σ) = (trS N s).substT (trσ N σ) := by
  simp only [trS, TSch.subst, Flick.Scheme.substT, trTT_subst, Flick.Scheme.mk.injEq, true_and]
  rw [funext (trσ_lift N s.k σ)]
  refine ⟨?_, rfl⟩
  rw [← funext (trσ_lift N s.k σ)]
  unfold trFacts; split
  · exact (Facts.none_substT _).symm
  · exact (trBd_substT N _ _).symm

theorem trS_up (s : TSch) (k : Nat) : trS N (s.subst (up k)) = (trS N s).up k := by
  rw [trS_subst, Flick.Scheme.up, funext (trσ_up N k)]

theorem trD_up (q : Cn) (k : Nat) :
    Flick.Scheme.mono (trD N (q.subst (up k))) = (Flick.Scheme.mono (trD N q)).up k := by
  simp only [Flick.Scheme.up, Flick.Scheme.substT, Flick.Scheme.mono, Facts.none_substT,
    trD_subst, liftT_zero]
  rw [funext (trσ_up N k)]

/-- A closed type is an instance of its translation: `⟦t⟧[⟦σ 0⟧ … ⟦σ (k-1)⟧] = ⟦t σ⟧`. -/
theorem trTT_inst {k : Nat} {t : TTy} (h : t.Closed k) (σ : Nat → Ty) :
    (trTT N t).substT (instT ((List.range k).map fun j => trT N (σ j))) = trTT N (t.subst σ) := by
  have e : (List.range k).map (fun j => trT N (σ j)) = ((List.range k).map σ).map (trT N) := by
    simp [List.map_map, Function.comp_def]
  rw [e, ← funext (trσ_inst N _), ← trTT_subst]
  congr 1
  apply h
  intro j hj
  simp [inst, hj]

/-- Translate elaboration variance to core variance. -/
def trVar : Variance → Flick.Variance
  | .pos => .pos
  | .neg => .neg
  | .inv => .inv
  | .bi => .bi

/-- `⟦T ū⟧ = T ⟦ū⟧` for nominal data constructors (`T ≠ 2, 4`). -/
theorem trT_foldl_app {N : Names} (us : List Ty) (f : Ty) :
    trT N (us.foldl .app f) = (us.map (trT N)).foldl .tapp (trT N f) := by
  induction us generalizing f with
  | nil => rfl
  | cons u us ih => exact ih (.app f u)

theorem trT_tcon {N : Names} {T : Nat} (h2 : T ≠ 2) (h4 : T ≠ 4) (us : List Ty) :
    trT N (Ty.tcon T us) = Flick.Ty.tcon (N.con T) (us.map (trT N)) := by
  rw [Ty.tcon, trT_foldl_app]
  simp [trT, h2, h4, Flick.Ty.tcon]

/-- Service-row inclusion translates to core `Row.Sub`. -/
theorem RowLe.tr {r₁ r₂ : Ty} (h : RowLe r₁ r₂) :
    Row.Sub (trT N r₁).asRow (trT N r₂).asRow := by
  induction h with
  | refl => exact fun _ hx => hx
  | cnil =>
    intro x hx
    cases x with
    | atm a => exact hx.elim
    | _ => exact hx.elim
  | runL _ _ ih₁ ih₂ =>
    intro x hx
    rcases hx with hx | hx
    · exact ih₁ x hx
    · exact ih₂ x hx
  | runRL _ ih => exact fun x hx => .inl (ih x hx)
  | runRR _ ih => exact fun x hx => .inr (ih x hx)
  | keeps hk =>
    intro x hx
    cases x with
    | atm a =>
      rcases hx with hq | h0
      · obtain ⟨n, rfl, hn⟩ := (trT_reach N _ a).1 hq
        exact trT_keeps N (hk n hn)
      · exact h0.elim
    | _ => exact hx.elim

/-- Arrow use-stage comparison translates to core `Stage.le`. -/
theorem Stage.UseLe.tr {φ₁ φ₂ : Stage} {q₁ q₂ : Ty} (h : Stage.UseLe φ₁ q₁ φ₂ q₂) :
    (Stage.use (trStage N φ₁) (trT N q₁).reach).le
      (Stage.use (trStage N φ₂) (trT N q₂).reach) := by
  cases φ₁ with
  | P => trivial
  | R r₁ e₁ =>
    cases φ₂ with
    | P => exact h.elim
    | R r₂ e₂ =>
      obtain ⟨hr, rfl, hq⟩ := h
      refine ⟨fun x hx => ?_, fun _ he => he⟩
      rcases hx with hx | hx
      · exact .inl (hr.tr N x hx)
      · cases x with
        | atm a =>
          obtain ⟨n, rfl, hn⟩ := (trT_reach N q₁ a).1 hx
          rcases hq n hn with h2 | hk
          · exact .inr (trT_reach_v N h2)
          · exact .inl (trT_keeps N hk)
        | _ => exact hx.elim

end Flick.Elab
