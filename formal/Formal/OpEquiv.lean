import Formal.AnswerSubst

/-!
# Operations at the arguments a handler answers

A handler answers its effect by name, so the operations that reach it must be performed at
arguments at which their types are those of its arms. When `E ū` and `E v̄`, with as many
arguments as `E` has parameters, are congruent, so are their arguments one by one, and so are the
types of each operation at them (`Ty.Equiv.at_tcon`). A handler at `us` answers only such
arguments (`Sig.answers_equiv`).
-/

namespace Flick

variable {S : Sig}

theorem Ty.Equiv.foldl_tapp : ∀ {vs us : List Ty} {f g : Ty}, vs.length = us.length →
    Ty.Equiv (vs.foldl .tapp f) (us.foldl .tapp g) →
      Ty.Equiv f g ∧ ∀ i (hi : i < vs.length) (hj : i < us.length), Ty.Equiv vs[i] us[i]
  | [], [], _, _, _, h => ⟨h, fun _ hi => by simp at hi⟩
  | v :: vs, u :: us, f, g, hl, h => by
    obtain ⟨hfg, hrest⟩ := Ty.Equiv.foldl_tapp (vs := vs) (us := us) (f := .tapp f v) (g := .tapp g u)
      (by simpa using hl) h
    have hfa : Ty.Equiv f g ∧ Ty.Equiv v u := by
      cases hfg.form with
      | rows hr _ => cases hr
      | tapp hf ha => exact ⟨hf, ha⟩
    refine ⟨hfa.1, fun i hi hj => ?_⟩
    cases i with
    | zero => exact hfa.2
    | succ i => exact hrest i (by simpa using hi) (by simpa using hj)
  | [], _ :: _, _, _, hl, _ | _ :: _, [], _, _, hl, _ => by simp at hl

/-- The arguments of congruent instances, one by one, as a substitution of the parameters. -/
theorem Ty.Equiv.instT_tcon {E : Name} {vs us : List Ty} (hl : vs.length = us.length)
    (h : Ty.Equiv (Ty.tcon E vs) (Ty.tcon E us)) : ∀ x, Ty.Equiv (instT vs x) (instT us x) := by
  have := (Ty.Equiv.foldl_tapp hl h).2
  rintro (i | p)
  · by_cases hi : i < vs.length
    · simp only [instT, hi, dif_pos, show i < us.length by omega]
      exact this i hi (by omega)
    · simp only [instT, dif_neg, show ¬ i < us.length by omega, not_false_eq_true, hl]
      exact .refl
  · exact .refl

/-- A type over the parameters of `E`, as the type of an operation, is congruent at congruent
instances. -/
theorem Ty.Equiv.at_tcon {E : Name} {vs us : List Ty} (a : Ty) (hl : vs.length = us.length)
    (h : Ty.Equiv (Ty.tcon E vs) (Ty.tcon E us)) :
    Ty.Equiv (a.substT (instT vs)) (a.substT (instT us)) :=
  Ty.substT_equiv a (Ty.Equiv.instT_tcon hl h)

/-- An instance a handler at `us` answers is congruent to `E us`. -/
theorem Sig.answers_equiv {E : Name} {us vs : List Ty} (hus : us.length = S.arity E)
    (hvs : vs.length = S.arity E) (h : S.answers E us (Ty.tcon E vs).classOf) :
    Ty.Equiv (Ty.tcon E vs) (Ty.tcon E us) := by
  rcases h with h | h
  · rw [h] at hus hvs
    rw [List.length_eq_zero_iff.mp hus, List.length_eq_zero_iff.mp hvs]
    exact .refl
  · exact Ty.classOf_eq.mp h

theorem Ty.tcon_inj {E : Name} {us vs : List Ty} (h : Ty.tcon E us = Ty.tcon E vs) : us = vs := by
  have key : ∀ xs ys : List Ty,
      xs.reverse.foldl Ty.tapp (Ty.adt E) = ys.reverse.foldl Ty.tapp (Ty.adt E) → xs = ys := by
    intro xs
    induction xs with
    | nil => intro ys h; cases ys <;> simp_all [List.foldl_append]
    | cons x xs ih =>
      intro ys h
      cases ys with
      | nil => simp [List.foldl_append] at h
      | cons y ys =>
        simp only [List.reverse_cons, List.foldl_append, List.foldl_cons, List.foldl_nil,
          Ty.tapp.injEq] at h
        rw [ih ys h.1, h.2]
  have := key us.reverse vs.reverse (by simpa [Ty.tcon] using h)
  simpa using congrArg List.reverse this

/-- The arguments of an instance a type fixed by a substitution names are fixed by it. -/
theorem Ty.tcon_fixed {E : Name} {us : List Ty} {σ : Atom → Ty}
    (h : (Ty.tcon E us).substT σ = Ty.tcon E us) : us.map (Ty.substT σ) = us :=
  Ty.tcon_inj (by rw [← Ty.tcon_substT]; exact h)

/-- The types of an operation at arguments a substitution fixes are fixed by it. -/
theorem Sig.op_inst_fixed {E o : Name} {c : OpClass} {a b : Ty} {us : List Ty} {σ : Atom → Ty}
    (hop : S.op E o = some (c, a, b)) (hus : us.length = S.arity E)
    (h : (Ty.tcon E us).substT σ = Ty.tcon E us) :
    (a.substT (instT us)).substT σ = a.substT (instT us) ∧
      (b.substT (instT us)).substT σ = b.substT (instT us) := by
  obtain ⟨ha, hb⟩ := S.op_inst_substT hop hus σ
  rw [Ty.tcon_fixed h] at ha hb
  exact ⟨ha, hb⟩

end Flick
