import Formal.Elab.CoreEval

/-!
# λFlick⁺ as a `Target`: the run of dictionaries

`trDt N δ d` is the translation of a dictionary, a function of the term, which carries its type (`TrD.eq`).
`RealRun P N root` is what the bodies of the globals do: the global of an instance is
`λ^l. drec …` whose slot `N.slot i` is the translation of the slot over the premises, that of
`struct_C^τ` has the slot `struct_S^τ (δ̄.$slotᵢ)` (`docs/formal/elaboration.md#equivalence-did-not-relate-structural-slots-to-superclass-dictionaries`), a primitive one is a value, and the
type of a structural or primitive global is erased (`docs/formal/elaboration.md#the-structural-dictionary-term-carries-its-type`).

Under it a closed derivation evaluates (`ent_eval`): an instance applied to the values of its
arguments is a record value, without projecting a slot. `≈` keeps the values of a dictionary
(`eqv_sem`): the slot rules take closed derivations (`docs/formal/elaboration.md#slot-rules-of-equivalence-take-closed-derivations`), so their arguments have values,
and the translation of a dictionary is a term of `var`, `glob`, application and projection,
whose values depend only on the values of its variables (`trDt_sem`).
-/

namespace Flick.Elab

variable (N : Names)

variable {N}

theorem trDt_apps (δ : Nat → Nat) (f : Dict) (ds : List Dict) :
    trDt N δ (apps f ds) = appsC (trDt N δ f) (ds.map (trDt N δ)) := by
  induction ds generalizing f with
  | nil => rfl
  | cons d ds ih => exact ih (.ap f d)


variable {root : Roots}

theorem atom_plug {E : List Frame} {e a : Flick.Tm} (ha : ∀ (F : Frame) x, F.plug x ≠ a)
    (h : plug E e = a) : E = [] ∧ e = a := by
  rcases snoc_or E with rfl | ⟨E₀, F, rfl⟩
  · exact ⟨rfl, h⟩
  · rw [plug_snoc] at h; exact absurd h (ha _ _)

theorem var_stuck {j : Nat} {b : Flick.Tm} (h : PStep root (.var j) b) : False := by
  obtain ⟨E, e, _, _, _, hq, -, hs⟩ := h.inv
  obtain ⟨rfl, rfl⟩ := atom_plug (fun F x => by cases F <;> simp [Frame.plug]) hq.symm
  rcases hs with hs | ⟨g, hg, -⟩
  · cases hs
  · cases hg

theorem var_sem (i j : Nat) : SemEq root (.var i) (.var j) := by
  have ne : ∀ k v, ¬ Eval root (.var k) v := by
    rintro k v ⟨s, hv⟩
    cases s with
    | refl => cases hv
    | step s _ => exact var_stuck s
  exact fun v => ⟨fun h => (ne _ _ h).elim, fun h => (ne _ _ h).elim⟩

theorem glob_step {g : Nat} {b : Flick.Tm} (h : PStep root (.glob g) b) : root.glob g = some b := by
  obtain ⟨E, e, e', _, _, hq, rfl, hs⟩ := h.inv
  obtain ⟨rfl, rfl⟩ := atom_plug (fun F x => by cases F <;> simp [Frame.plug]) hq.symm
  rcases hs with hs | ⟨g', hg, hb⟩
  · cases hs
  · cases hg; exact hb

theorem _root_.Flick.PStep.glob1 {g : Nat} {v : Flick.Tm} (h : root.glob g = some v) :
    PStep root (.glob g) v := PStep.glob (E := []) OkE.nil h

/-- Two globals with one body have the same values. -/
theorem glob_sem {g g' : Nat} (h : root.glob g = root.glob g') : SemEq root (.glob g) (.glob g') := by
  cases hb : root.glob g with
  | some B =>
    rw [hb] at h
    exact fun v => (Eval.of_step (.glob1 hb)).trans (Eval.of_step (.glob1 h.symm)).symm
  | none =>
    rw [hb] at h
    have ne : ∀ g₀, root.glob g₀ = none → ∀ v, ¬ Eval root (.glob g₀) v := by
      rintro g₀ h₀ v ⟨s, hv⟩
      cases s with
      | refl => cases hv
      | step s _ => rw [glob_step s] at h₀; cases h₀
    exact fun v => ⟨fun e => (ne _ hb v e).elim, fun e => (ne _ h.symm v e).elim⟩

theorem semEq_proj {a b : Flick.Tm} (h : SemEq root a b) (l : Label) :
    SemEq root (.proj a l) (.proj b l) := by
  intro v
  refine (eval_frame (F := .proj l) trivial (a := a)).trans
    (Iff.trans ?_ (eval_frame (F := .proj l) trivial (a := b)).symm)
  exact ⟨fun ⟨w, hw, e⟩ => ⟨w, (h w).1 hw, e⟩, fun ⟨w, hw, e⟩ => ⟨w, (h w).2 hw, e⟩⟩

theorem eval_app {a b v : Flick.Tm} :
    Eval root (.app a b) v ↔ ∃ w u, Eval root a w ∧ Eval root b u ∧ Eval root (.app w u) v := by
  refine (eval_frame (F := .appL b) trivial (a := a)).trans ?_
  constructor
  · rintro ⟨w, hw, e⟩
    obtain ⟨u, hu, e'⟩ := (eval_frame (F := .appR w) hw.2).1 e
    exact ⟨w, u, hw, hu, e'⟩
  · rintro ⟨w, u, hw, hu, e⟩
    exact ⟨w, hw, (eval_frame (F := .appR w) hw.2).2 ⟨u, hu, e⟩⟩

theorem semEq_app {a a' b b' : Flick.Tm} (ha : SemEq root a a') (hb : SemEq root b b') :
    SemEq root (.app a b) (.app a' b') := by
  intro v
  rw [eval_app, eval_app]
  exact ⟨fun ⟨w, u, hw, hu, e⟩ => ⟨w, u, (ha w).1 hw, (hb u).1 hu, e⟩,
    fun ⟨w, u, hw, hu, e⟩ => ⟨w, u, (ha w).2 hw, (hb u).2 hu, e⟩⟩

/-- The values of a translated dictionary depend only on the values of its variables. -/
theorem trDt_sem (δ : Nat → Nat) {σ σ' : Nat → Flick.Tm} (h : ∀ j, SemEq root (σ j) (σ' j)) :
    ∀ s, SemEq root ((trDt N δ s).subst σ) ((trDt N δ s).subst σ') := by
  intro s
  induction s with
  | var k => exact h _
  | slot d i ih => exact semEq_proj ih _
  | ap d e ihd ihe => exact semEq_app ihd ihe
  | _ => exact SemEq.refl _

theorem Tm.field_subst {r t : Flick.Tm} {l : Label} (h : r.field l = some t) (σ : Nat → Flick.Tm) :
    (r.subst σ).field l = some (t.subst σ) := by
  induction r with
  | rcons l' e rest _ ih =>
    simp only [Flick.Tm.field] at h ⊢
    simp only [Flick.Tm.subst, Flick.Tm.field]
    split
    · rw [if_pos ‹_›] at h; cases h; rfl
    · rw [if_neg ‹_›] at h; exact ih h
  | drec c r ih => exact ih h
  | _ => simp [Flick.Tm.field] at h

/-- A global `λ^n. drec c r` applied to arguments with values. -/
theorem eval_body {g n c : Nat} {r : Flick.Tm} (hg : root.glob g = some (lamsC n (.drec c r)))
    {args vs : List Flick.Tm} (h : All2 (Eval root) args vs) (hn : args.length = n) :
    Eval root (appsC (.glob g) args) (.drec c (r.subst (Flick.inst vs.reverse))) := by
  have h' : All2 (fun a v => Star (PStep root) a v ∧ Value v) args vs := by
    clear hn; induction h with
    | nil => exact .nil
    | cons e _ ih => exact .cons e ih
  refine ⟨.step ((PStep.glob1 hg).apps args) ?_, .drec⟩
  subst hn
  exact beta_chain h' _

theorem eval_proj {c : Nat} {r w v : Flick.Tm} {l : Label} (h : r.field l = some w) :
    Eval root (.proj (.drec c r) l) v ↔ Eval root w v :=
  Eval.of_step (PStep.head (E := []) OkE.nil (.proj .drec h))

end Flick.Elab
