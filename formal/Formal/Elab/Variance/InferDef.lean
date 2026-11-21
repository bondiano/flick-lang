import Formal.Elab.Variance.Pol

/-!
# Computable class-parameter variance inference definitions (`Formal.Elab.Variance.InferDef`)
-/

namespace Flick.Elab

/-! ### Computable class-parameter variance inference and soundness -/

/-- Lattice join on variances (`bi ⊑ {pos, neg} ⊑ inv`). -/
def Variance.join : Variance → Variance → Variance
  | .bi, v => v
  | v, .bi => v
  | .pos, .pos => .pos
  | .neg, .neg => .neg
  | _, _ => .inv

/-- Variance order `v₁ ⊑ v₂` (`bi ⊑ pos, neg ⊑ inv`). -/
def Variance.Le : Variance → Variance → Prop
  | .bi, _ => True
  | _, .inv => True
  | .pos, .pos => True
  | .neg, .neg => True
  | _, _ => False

theorem Variance.le_refl (v : Variance) : v.Le v := by cases v <;> trivial

theorem Variance.le_join_left {a b c : Variance} (h : (a.join b).Le c) : a.Le c := by
  cases a <;> cases b <;> cases c <;> simp [Variance.join, Variance.Le] at h ⊢

theorem Variance.le_join_right {a b c : Variance} (h : (a.join b).Le c) : b.Le c := by
  cases a <;> cases b <;> cases c <;> simp [Variance.join, Variance.Le] at h ⊢

theorem Variance.le_bi {v : Variance} (h : v.Le .bi) : v = .bi := by
  cases v <;> simp [Variance.Le] at h ⊢

theorem Variance.le_pos_of {v : Variance} (h : v.Le .pos) : v = .bi ∨ v = .pos := by
  cases v <;> simp [Variance.Le] at h ⊢

theorem Variance.le_neg_of {v : Variance} (h : v.Le .neg) : v = .bi ∨ v = .neg := by
  cases v <;> simp [Variance.Le] at h ⊢

mutual
/-- Decidable check for `Ty.Occurs`. -/
def Ty.occursB (n : Nat) : Ty → Bool
  | .var m => m == n
  | .con _ => false
  | .app t u => t.occursB n || u.occursB n
  | .arr φ a b q => a.occursB n || b.occursB n || q.occursB n || φ.occursB n
  | .cnil => false
  | .cun q q' => q.occursB n || q'.occursB n
  | .rnm _ t => t.occursB n
  | .run r r' => r.occursB n || r'.occursB n
  | .all k t => t.occursB (n + k)

def Stage.occursB (n : Nat) : Stage → Bool
  | .P => false
  | .R r e => r.occursB n || e.occursB n
end

theorem Ty.occursB_iff (n : Nat) (t : Ty) : t.occursB n = true ↔ t.Occurs n := by
  induction t using Ty.ind generalizing n with
  | var m => simp [Ty.occursB, Ty.Occurs]
  | con _ | cnil => simp [Ty.occursB, Ty.Occurs]
  | app _ _ iht ihu | cun _ _ iht ihu | run _ _ iht ihu =>
    simp [Ty.occursB, Ty.Occurs, iht, ihu]
  | rnm _ _ iht => simp [Ty.occursB, Ty.Occurs, iht]
  | all _ _ iht => simp [Ty.occursB, Ty.Occurs, iht]
  | arr φ _ _ _ iha ihb ihq ihφ =>
    cases φ with
    | P =>
      simp only [Ty.occursB, Stage.occursB, Ty.Occurs, Stage.Occurs, Bool.or_eq_true,
        iha, ihb, ihq, Bool.false_eq_true, or_false, or_assoc]
    | R _ _ =>
      simp only [Ty.occursB, Stage.occursB, Ty.Occurs, Stage.Occurs, Bool.or_eq_true,
        iha, ihb, ihq, ihφ.1, ihφ.2, or_assoc]

theorem Stage.occursB_iff (n : Nat) (φ : Stage) : φ.occursB n = true ↔ φ.Occurs n := by
  cases φ with
  | P => simp [Stage.occursB, Stage.Occurs]
  | R r e => simp [Stage.occursB, Stage.Occurs, Ty.occursB_iff]

theorem sizeOf_foldl_app : ∀ (us : List Ty) (f : Ty),
    sizeOf (us.foldl .app f) + 1 = sizeOf f + sizeOf us
  | [], f => by simp
  | x :: xs, f => by
    have ih := sizeOf_foldl_app xs (.app f x)
    simp only [List.foldl_cons, Ty.app.sizeOf_spec, List.cons.sizeOf_spec] at ih ⊢
    omega

theorem sizeOf_le_tcon (T : Nat) (us : List Ty) : sizeOf us ≤ sizeOf (Ty.tcon T us) := by
  have h := sizeOf_foldl_app us (.con T)
  simp only [Ty.tcon, Ty.con.sizeOf_spec] at h ⊢
  omega

mutual
/-- Infer the required variance of parameter `i` in type `t` at polarity `q` (`true`: covariant),
given data-constructor variances `dvar`.
- Unused (`¬ t.occursB i`): `.bi`
- Type variable `var i` at polarity `q`: `.pos` (`q = true`) or `.neg` (`q = false`)
- Arrow `a →φ^c b`: `φ` invariant; `a` flipped (under a double flip, `.inv` because reach
  preservation forbids `+` below a flip); `b` at `q`; `c` at `q`
- Data constructor `T ū` (`T ≠ 2, 4`): argument `j` composed with `dvar T j` (double flip → `.inv`)
- Higher-kinded application (`f a` where `f` is not a constructor), `all`, rows: `.inv` if `i` occurs. -/
def inferTyVar (dvar : Nat → Nat → Variance) (i : Nat) (q : Bool) (t : Ty) : Variance :=
  if !t.occursB i then .bi
  else match t with
  | .var m => if m == i then (if q then .pos else .neg) else .bi
  | .arr φ a b c =>
    let vφ : Variance := if φ.occursB i then .inv else .bi
    let va : Variance := if !q then (if a.occursB i then .inv else .bi) else inferTyVar dvar i (!q) a
    let vb : Variance := inferTyVar dvar i q b
    let vc : Variance := if c.occursB i then (if q then .pos else .neg) else .bi
    vφ.join (va.join (vb.join vc))
  | .app f u =>
    match _h : (Ty.app f u).unspine with
    | some (T, us) =>
      if T == 2 || T == 4 then .inv
      else inferTyVarArgs dvar i q T 0 us
    | none => .inv
  | _ => .inv
termination_by (sizeOf t, 1)
decreasing_by
  all_goals simp_wf
  · left; omega
  · left; omega
  · have hle := sizeOf_le_tcon T us
    rw [← Ty.unspine_eq _h] at hle
    by_cases hlt : sizeOf us < sizeOf (Ty.app f u)
    · exact Prod.Lex.left _ _ hlt
    · have heq : sizeOf us = sizeOf (Ty.app f u) := by omega
      rw [heq]; exact Prod.Lex.right _ (by omega)

def inferTyVarArgs (dvar : Nat → Nat → Variance) (i : Nat) (q : Bool) (T : Nat) (j : Nat) :
    List Ty → Variance
  | [] => .bi
  | x :: xs =>
    let vx : Variance :=
      match dvar T j with
      | .pos => inferTyVar dvar i q x
      | .neg =>
        if !q then (if x.occursB i then .inv else .bi)
        else inferTyVar dvar i (!q) x
      | .inv => if x.occursB i then .inv else .bi
      | .bi => if x.occursB i then (if q then .pos else .neg) else .bi
    vx.join (inferTyVarArgs dvar i q T (j + 1) xs)
termination_by xs => (sizeOf xs, 0)
decreasing_by
  all_goals simp_wf
  · left; omega
  · left; omega
  · left; omega
end

/-- Infer the required variance of parameter `i` in a constraint `c` at polarity `q`
(`true` for a superclass, `false` for a method's own constraint). -/
def inferCnVar (dvar cvar : Nat → Nat → Variance) (i : Nat) (q : Bool) (c : Cn) : Variance :=
  let rec go (j : Nat) : List Ty → Variance
    | [] => .bi
    | x :: xs =>
      let vx : Variance :=
        match cvar c.cls j with
        | .pos => inferTyVar dvar i q x
        | .neg =>
          if !q then (if x.occursB i then .inv else .bi)
          else inferTyVar dvar i (!q) x
        | .inv | .bi => if x.occursB i then .inv else .bi
      vx.join (go (j + 1) xs)
  go 0 c.args

/-- Infer the required variance of class parameter `i` from a method with extra constraints
`rest` and type `ty`. -/
def inferMethodVar (dvar cvar : Nat → Nat → Variance) (i : Nat) (rest : List Cn) (ty : Ty) :
    Variance :=
  rest.foldl (fun acc c => acc.join (inferCnVar dvar cvar i false c)) (inferTyVar dvar i true ty)

/-- Infer the variance of parameter `i` of a class from its superclasses and methods. -/
def inferClassVar (dvar cvar : Nat → Nat → Variance) (i : Nat) (supers : List Cn)
    (methods : List (List Cn × Ty)) : Variance :=
  let vs := supers.foldl (fun acc c => acc.join (inferCnVar dvar cvar i true c)) .bi
  methods.foldl (fun acc m => acc.join (inferMethodVar dvar cvar i m.1 m.2)) vs

theorem inferTyVar_bi {dvar : Nat → Nat → Variance} {i : Nat} {q : Bool} {t : Ty} :
    inferTyVar dvar i q t = .bi → ¬ t.Occurs i := by
  intro h
  unfold inferTyVar at h
  by_cases ho : t.occursB i = true
  · simp only [ho, Bool.not_true, Bool.false_eq_true, ↓reduceIte] at h
    cases ht : t with
    | var m =>
      rw [ht] at ho h
      have hm : m = i := by simpa [Ty.occursB] using ho
      subst hm
      cases q <;> simp at h
    | con _ | cnil => rw [ht] at ho; simp [Ty.occursB] at ho
    | arr φ a b c =>
      have hlt_a : sizeOf a < sizeOf t := by rw [ht]; simp; omega
      have hlt_b : sizeOf b < sizeOf t := by rw [ht]; simp; omega
      rw [ht] at h; dsimp only at h
      have h1 := Variance.le_bi (Variance.le_join_left (by rw [h]; exact Variance.le_refl .bi))
      have h2 := Variance.le_bi (Variance.le_join_left (Variance.le_join_right (by rw [h]; exact Variance.le_refl .bi)))
      have h3 := Variance.le_bi (Variance.le_join_left (Variance.le_join_right (Variance.le_join_right (by rw [h]; exact Variance.le_refl .bi))))
      have h4 := Variance.le_bi (Variance.le_join_right (Variance.le_join_right (Variance.le_join_right (by rw [h]; exact Variance.le_refl .bi))))
      have hφ : ¬ φ.Occurs i := by
        intro hx; have hb := (Stage.occursB_iff i φ).2 hx; simp [hb] at h1
      have hc : ¬ c.Occurs i := by
        intro hx; have hb := (Ty.occursB_iff i c).2 hx; cases q <;> simp [hb] at h4
      have hb : ¬ b.Occurs i := inferTyVar_bi h3
      have ha : ¬ a.Occurs i := by
        cases q with
        | false =>
          intro hx; have hx' := (Ty.occursB_iff i a).2 hx; simp [hx'] at h2
        | true => exact inferTyVar_bi h2
      rintro (hx | hx | hx | hx)
      · exact ha hx
      · exact hb hx
      · exact hc hx
      · exact hφ hx
    | app f u =>
      rw [ht] at h; dsimp only at h
      cases hus : (Ty.app f u).unspine with
      | none => rw [hus] at h; cases h
      | some p =>
        obtain ⟨T, us⟩ := p
        rw [hus] at h
        by_cases h24 : (T == 2 || T == 4) = true
        · simp [h24] at h
        · simp only [h24, Bool.false_eq_true, ↓reduceIte] at h
          have heq := Ty.unspine_eq hus
          rw [heq]
          intro hocc
          obtain ⟨j, x, hj, hx⟩ := Ty.occurs_tcon_iff.mp hocc
          have hgo : ∀ (k : Nat) (xs : List Ty),
              inferTyVarArgs dvar i q T k xs = .bi →
              (∀ (m : Nat) (y : Ty), xs[m]? = some y → sizeOf y < sizeOf t) →
              ∀ (m : Nat) (y : Ty), xs[m]? = some y → ¬ y.Occurs i := by
            intro k xs
            induction xs generalizing k with
            | nil => intro _ _ _ _ hm; simp at hm
            | cons z zs ih =>
              intro hg hsz m y hm
              unfold inferTyVarArgs at hg
              have hz := Variance.le_bi (Variance.le_join_left (by rw [hg]; exact Variance.le_refl .bi))
              have hzs := Variance.le_bi (Variance.le_join_right (by rw [hg]; exact Variance.le_refl .bi))
              cases m with
              | zero =>
                simp at hm; subst hm
                have hsz0 : sizeOf z < sizeOf t := hsz 0 z rfl
                cases hd : dvar T k with
                | pos => rw [hd] at hz; exact inferTyVar_bi hz
                | neg =>
                  rw [hd] at hz
                  cases q with
                  | false =>
                    intro hy; have := (Ty.occursB_iff i z).2 hy; simp [this] at hz
                  | true => exact inferTyVar_bi hz
                | inv =>
                  rw [hd] at hz
                  intro hy; have := (Ty.occursB_iff i z).2 hy; simp [this] at hz
                | bi =>
                  rw [hd] at hz
                  intro hy; have := (Ty.occursB_iff i z).2 hy; cases q <;> simp [this] at hz
              | succ m =>
                exact ih (k + 1) hzs (fun n w hn => hsz (n + 1) w (by simpa using hn)) m y (by simpa using hm)
          exact hgo 0 us h (fun m y hm => by have := Ty.sizeOf_lt_tcon (T := T) hm; rw [← heq, ← ht] at this; exact this) j x hj hx
    | cun _ _ | rnm _ _ | run _ _ | all _ _ => rw [ht] at h; cases h
  · exact fun hx => ho ((Ty.occursB_iff i t).2 hx)
termination_by sizeOf t
decreasing_by
  all_goals simp_wf
  all_goals first | exact hlt_a | exact hlt_b | exact hsz0

end Flick.Elab
