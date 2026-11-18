import Formal.TypeEquivSubst
import Formal.TypeEquivView

/-!
# Named row elements keep their arguments

A named element of a row stands for its name at its argument up to congruence
(`Ty.asRow`), so congruence never identifies `Store Int` with `Store Bool`: not as rows,
not after a substitution, not under an invariant type such as `Ref`, and not under a
binder, which lifted substitution protects. Subtraction `rless G` still removes a name at
every argument.
-/

namespace Flick

/-- Two named elements are congruent exactly when their arguments are. -/
theorem Ty.Equiv.rnm_iff {N : Name} {a b : Ty} : Ty.Equiv (.rnm N a) (.rnm N b) ↔ Ty.Equiv a b := by
  refine ⟨fun h => ?_, .rnm⟩
  have hi : (Ty.rnm N a).asRow (.nm N a.classOf) := .inl rfl
  have hb := Eq.mp (congrFun h.asRow (.nm N a.classOf)) hi
  have he : a.classOf = b.classOf := by
    simpa [Ty.asRow, Row.union, Row.inst, Row.ofCaps] using hb
  exact Ty.classOf_eq.mp he

theorem Ty.rnm_int_not_bool (N : Name) :
    ¬ Ty.Equiv (.rnm N (.base .int)) (.rnm N (.base .bool)) := fun h =>
  nomatch (Ty.Equiv.rnm_iff.mp h).base_inj

/-- `S α`: the element of the service or error `S` at its one argument `α`. -/
def Ty.nm1 (S : Name) (a : Ty) : Ty := .rnm S (.tapp (.adt S) a)

theorem Ty.nm1_int_not_bool (S : Name) : ¬ Ty.Equiv (Ty.nm1 S (.base .int)) (Ty.nm1 S (.base .bool)) :=
  fun h => by
    have h := Ty.Equiv.rnm_iff.mp h
    cases h.form with
    | rows hr _ => cases hr
    | tapp _ ha => exact nomatch ha.base_inj

/-- A row that holds `S Int` does not hold `S Bool`, and congruence keeps what a row holds. -/
theorem Ty.Equiv.row_int_not_bool {S : Name} {r : Ty}
    (h : Ty.Equiv (.run (Ty.nm1 S (.base .int)) r) (.run (Ty.nm1 S (.base .bool)) r))
    (hr : ¬ r.asRow (.nm S (Ty.tapp (.adt S) (.base .int)).classOf)) : False := by
  have hi : (Ty.run (Ty.nm1 S (.base .int)) r).asRow (.nm S (Ty.tapp (.adt S) (.base .int)).classOf) :=
    .inl (.inl rfl)
  rcases Eq.mp (congrFun h.asRow _) hi with (hb | hb) | hb
  · have := Ty.classOf_eq.mp (Elem.nm.inj hb).2
    exact Ty.nm1_int_not_bool S (.rnm this)
  · exact hb
  · exact hr hb

/-- The stage of a function that performs `S Int` is not that of one performing `S Bool`, even
inside the invariant `Ref`. -/
theorem Ty.ref_int_not_bool (S : Name) (a b : Ty) (C : Caps) :
    ¬ Ty.Equiv (.ref (.arrow a (.rt (Ty.nm1 S (.base .int)) .cnil) b C))
      (.ref (.arrow a (.rt (Ty.nm1 S (.base .bool)) .cnil) b C)) := by
  intro h
  cases h.form with
  | rows hr _ => cases hr
  | ref h =>
    cases h.form with
    | rows hr _ => cases hr
    | arrow _ hφ _ =>
      have := congrArg Stage.svc hφ.asStage
      have hi : (Ty.nm1 S (.base .int)).asRow (.nm S (Ty.tapp (.adt S) (.base .int)).classOf) :=
        .inl rfl
      have hb := Eq.mp (congrFun this _) hi
      simp only [Ty.asStage, Stage.svc, Ty.nm1, Ty.asRow, Row.union, Row.inst, Row.ofCaps,
        Elem.nm.injEq, true_and, or_false] at hb
      exact Ty.nm1_int_not_bool S (.rnm (Ty.classOf_eq.mp hb))

/-- A substitution keeps them apart: the instance of `α` in `S α` is protected below a binder
by lifted substitution, so the bound `Ref (∀β. β →{S α} β)` at `Int` and at `Bool` differ. -/
theorem Ty.bound_int_not_bool (S : Name) :
    let body := Ty.all 1 (.arrow (.tvar 0) (.rt (Ty.nm1 S (.tvar 1)) .cnil) (.tvar 0) Caps.none)
    ¬ Ty.Equiv ((Ty.ref body).substT (instT [.base .int]))
      ((Ty.ref body).substT (instT [.base .bool])) := by
  intro body h
  simp only [body, Ty.substT, Ty.isStage, if_true, Ty.nm1, liftT, instT] at h
  simp at h
  cases h.form with
  | rows hr _ => cases hr
  | ref h =>
    cases h.form with
    | rows hr _ => cases hr
    | all h =>
      cases h.form with
      | rows hr _ => cases hr
      | arrow _ hφ _ =>
        have := congrArg Stage.svc hφ.asStage
        have hi : (Ty.rnm S (.tapp (.adt S) (.base .int))).asRow
            (.nm S (Ty.tapp (.adt S) (.base .int)).classOf) := .inl rfl
        have hb := Eq.mp (congrFun this _) (by simpa [Ty.up, Ty.ren, Atom.up, Atom.ty] using hi)
        simp only [Ty.asStage, Stage.svc, Ty.asRow, Row.union, Row.inst, Row.ofCaps,
          Elem.nm.injEq, true_and, or_false] at hb
        have := Ty.classOf_eq.mp hb
        simp only [Ty.up, Ty.ren] at this
        exact Ty.nm1_int_not_bool S (.rnm this)

/-- Subtraction is by name: `-S` removes `S` at every argument, before and after a
substitution. -/
theorem Ty.rless_removes_all (S : Name) (r : Ty) (σ : Atom → Ty) (c : TyClass) :
    ¬ ((Ty.rless (· = S) r).substT σ).asRow (.nm S c) := by
  simp [Ty.substT, Ty.asRow, Row.less]

end Flick
