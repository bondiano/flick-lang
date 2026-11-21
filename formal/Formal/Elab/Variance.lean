import Formal.Elab.Variance.InferSound

/-!
# Soundness of `inferCnVar` / `inferClassVar` and unfolding equations (`Formal.Elab.Variance`)
-/

namespace Flick.Elab

/-- **Soundness of computable constraint-variance inference**: if `inferCnVar dvar cvar i q c ⊑ v`
and `v ≠ inv`, then `v = bi` implies `i` occurs in no argument of `c`, and `v ∈ {pos, neg}`
implies `Cn.Pol dvar cvar i v q c`. -/
theorem inferCnVar_sound {dvar cvar : Nat → Nat → Variance} {i : Nat} {q : Bool} {c : Cn}
    {v : Variance} (hne : v ≠ .inv) (hle : (inferCnVar dvar cvar i q c).Le v) :
    (v = .bi → ∀ t ∈ c.args, ¬ t.Occurs i) ∧
    ((v = .pos ∨ v = .neg) → Cn.Pol dvar cvar i v q c) := by
  have hgo : ∀ (k : Nat) (xs : List Ty),
      (inferCnVar.go dvar cvar i q c k xs).Le v →
      (v = .bi → ∀ t ∈ xs, ¬ t.Occurs i) ∧
      ((v = .pos ∨ v = .neg) →
        (∀ (m : Nat) (u : Ty), xs[m]? = some u → cvar c.cls (k + m) = .pos → Ty.Pol dvar i v q u) ∧
        (∀ (m : Nat) (u : Ty), xs[m]? = some u → cvar c.cls (k + m) = .neg → v = .pos → ¬ u.Occurs i) ∧
        (∀ (m : Nat) (u : Ty), xs[m]? = some u → cvar c.cls (k + m) = .neg → Ty.Pol dvar i v (!q) u) ∧
        (∀ (m : Nat) (u : Ty), xs[m]? = some u → (cvar c.cls (k + m) = .inv ∨ cvar c.cls (k + m) = .bi) → ¬ u.Occurs i)) := by
    intro k xs
    induction xs generalizing k with
    | nil =>
      intro _
      refine ⟨fun _ _ ht => False.elim (List.not_mem_nil ht), fun _ =>
        ⟨fun _ _ hm => by simp at hm, fun _ _ hm => by simp at hm,
         fun _ _ hm => by simp at hm, fun _ _ hm => by simp at hm⟩⟩
    | cons z zs ih =>
      intro hg
      simp only [inferCnVar.go] at hg
      have hz := Variance.le_join_left hg
      have hzs := Variance.le_join_right hg
      obtain ⟨ihb, ihp⟩ := ih (k + 1) hzs
      refine ⟨fun hbi t ht => ?_, fun hpn => ?_⟩
      · rcases List.mem_cons.mp ht with rfl | ht
        · subst hbi
          cases hc : cvar c.cls k with
          | pos => rw [hc] at hz; exact (inferTyVar_sound (by decide) hz).1 rfl
          | neg =>
            rw [hc] at hz
            cases q with
            | false => intro hx; have hb := (Ty.occursB_iff i t).2 hx; simp [hb, Variance.Le] at hz
            | true => exact (inferTyVar_sound (by decide) hz).1 rfl
          | inv | bi =>
            rw [hc] at hz
            intro hx; have hb := (Ty.occursB_iff i t).2 hx; simp [hb, Variance.Le] at hz
        · exact ihb hbi t ht
      · obtain ⟨ip, ing, ineg, iinv⟩ := ihp hpn
        refine ⟨?_, ?_, ?_, ?_⟩
        · intro m u hm hc
          cases m with
          | zero =>
            simp at hm; subst hm; rw [Nat.add_zero] at hc; rw [hc] at hz
            exact (inferTyVar_sound hne hz).2 hpn
          | succ m =>
            rw [show k + (m + 1) = k + 1 + m by omega] at hc
            exact ip m u (by simpa using hm) hc
        · intro m u hm hc hv
          cases m with
          | zero =>
            simp at hm; subst hm; rw [Nat.add_zero] at hc; rw [hc] at hz
            subst hv
            cases q with
            | false => intro hx; have hb := (Ty.occursB_iff i z).2 hx; simp [hb, Variance.Le] at hz
            | true =>
              simp only [Bool.not_true, Bool.false_eq_true, ↓reduceIte] at hz
              rcases Variance.le_pos_of hz with hbi | hpos
              · exact inferTyVar_bi hbi
              · exact ((inferTyVar_sound (v := .pos) (by decide)
                  (by rw [hpos]; exact Variance.le_refl .pos)).2 (.inl rfl)).noOccurs (.inr ⟨rfl, rfl⟩)
          | succ m =>
            rw [show k + (m + 1) = k + 1 + m by omega] at hc
            exact ing m u (by simpa using hm) hc hv
        · intro m u hm hc
          cases m with
          | zero =>
            simp at hm; subst hm; rw [Nat.add_zero] at hc; rw [hc] at hz
            cases q with
            | false =>
              by_cases hzb : z.occursB i = true
              · simp only [hzb, ↓reduceIte] at hz
                cases v <;> simp [Variance.Le] at hz hne
              · exact .none fun hx => hzb ((Ty.occursB_iff i z).2 hx)
            | true =>
              simp only [Bool.not_true, Bool.false_eq_true, ↓reduceIte] at hz
              exact (inferTyVar_sound hne hz).2 hpn
          | succ m =>
            rw [show k + (m + 1) = k + 1 + m by omega] at hc
            exact ineg m u (by simpa using hm) hc
        · intro m u hm hc
          cases m with
          | zero =>
            simp at hm; subst hm; rw [Nat.add_zero] at hc
            rcases hc with hc | hc <;> rw [hc] at hz <;>
              (intro hx; have hb := (Ty.occursB_iff i z).2 hx; simp only [hb, ↓reduceIte] at hz;
                cases v <;> simp [Variance.Le] at hz hne)
          | succ m =>
            rw [show k + (m + 1) = k + 1 + m by omega] at hc
            exact iinv m u (by simpa using hm) hc
  obtain ⟨hb, hp⟩ := hgo 0 c.args hle
  exact ⟨hb, fun hpn => by simpa only [Nat.zero_add] using hp hpn⟩

private theorem foldl_join_le {α : Type} {f : α → Variance} {v : Variance} :
    ∀ {xs : List α} {acc : Variance},
      (xs.foldl (fun a x => a.join (f x)) acc).Le v →
      acc.Le v ∧ ∀ x ∈ xs, (f x).Le v
  | [], acc, h => ⟨h, fun _ hx => nomatch hx⟩
  | x :: xs, acc, h => by
    obtain ⟨h1, h2⟩ := foldl_join_le (xs := xs) (acc := acc.join (f x)) h
    refine ⟨Variance.le_join_left h1, fun y hy => ?_⟩
    rcases List.mem_cons.mp hy with rfl | hy
    · exact Variance.le_join_right h1
    · exact h2 y hy

/-- **Soundness of computable class-variance inference**: if a class's declared variance `v ≠ inv`
is an upper bound on `inferClassVar dvar cvar i supers methods`, then every superclass and every
method (both its type and its extra constraints) satisfies the polarity invariants of
`Prog.ClsVarOk`. -/
theorem inferClassVar_sound {dvar cvar : Nat → Nat → Variance} {i : Nat}
    {supers : List Cn} {methods : List (List Cn × Ty)} {v : Variance} (hne : v ≠ .inv)
    (hle : (inferClassVar dvar cvar i supers methods).Le v) :
    (∀ S ∈ supers, (v = .bi → ∀ t ∈ S.args, ¬ t.Occurs i) ∧
      ((v = .pos ∨ v = .neg) → Cn.Pol dvar cvar i v true S)) ∧
    (∀ m ∈ methods,
      (v = .bi → ¬ m.2.Occurs i ∧ ∀ c ∈ m.1, ∀ t ∈ c.args, ¬ t.Occurs i) ∧
      ((v = .pos ∨ v = .neg) →
        Ty.Pol dvar i v true m.2 ∧ ∀ c ∈ m.1, Cn.Pol dvar cvar i v false c)) := by
  unfold inferClassVar at hle
  obtain ⟨hsup, hmeth⟩ := foldl_join_le hle
  obtain ⟨-, hsups⟩ := foldl_join_le hsup
  refine ⟨fun S hS => inferCnVar_sound hne (hsups S hS), fun m hm => ?_⟩
  have hmi := hmeth m hm
  unfold inferMethodVar at hmi
  obtain ⟨hty, hcs⟩ := foldl_join_le hmi
  obtain ⟨htyb, htyp⟩ := inferTyVar_sound hne hty
  exact ⟨fun hbi => ⟨htyb hbi, fun c hc => (inferCnVar_sound hne (hcs c hc)).1 hbi⟩,
    fun hpn => ⟨htyp hpn, fun c hc => (inferCnVar_sound hne (hcs c hc)).2 hpn⟩⟩

/-! ### Unfolding equations for computable `inferTyVar` / `inferTyVarArgs` -/

theorem inferTyVar_of_false {dvar : Nat → Nat → Variance} {i : Nat} {q : Bool} {t : Ty}
    (h : t.occursB i = false) : inferTyVar dvar i q t = .bi := by
  unfold inferTyVar; simp [h]

theorem inferTyVar_var {dvar : Nat → Nat → Variance} {i : Nat} {q : Bool} {m : Nat} :
    inferTyVar dvar i q (.var m) = if m == i then (if q then .pos else .neg) else .bi := by
  unfold inferTyVar
  by_cases h : (m == i) = true <;> simp [Ty.occursB, h]

theorem inferTyVar_arr {dvar : Nat → Nat → Variance} {i : Nat} {q : Bool} {φ : Stage}
    {a b c : Ty} :
    inferTyVar dvar i q (.arr φ a b c) =
      if !((Ty.arr φ a b c).occursB i) then .bi
      else (if φ.occursB i then Variance.inv else .bi).join
        ((if !q then (if a.occursB i then Variance.inv else .bi) else inferTyVar dvar i (!q) a).join
          ((inferTyVar dvar i q b).join (if c.occursB i then (if q then Variance.pos else .neg) else .bi))) := by
  conv => lhs; unfold inferTyVar

theorem inferTyVar_app {dvar : Nat → Nat → Variance} {i : Nat} {q : Bool} {f u : Ty} :
    inferTyVar dvar i q (.app f u) =
      if !((Ty.app f u).occursB i) then .bi
      else match (Ty.app f u).unspine with
        | some (T, us) =>
          if T == 2 || T == 4 then .inv
          else inferTyVarArgs dvar i q T 0 us
        | none => .inv := by
  conv => lhs; unfold inferTyVar
  dsimp only
  split
  · rfl
  · cases (Ty.app f u).unspine <;> rfl

theorem inferTyVarArgs_nil {dvar : Nat → Nat → Variance} {i : Nat} {q : Bool} {T j : Nat} :
    inferTyVarArgs dvar i q T j [] = .bi := by
  conv => lhs; unfold inferTyVarArgs

theorem inferTyVarArgs_cons {dvar : Nat → Nat → Variance} {i : Nat} {q : Bool} {T j : Nat}
    {x : Ty} {xs : List Ty} :
    inferTyVarArgs dvar i q T j (x :: xs) =
      (match dvar T j with
        | .pos => inferTyVar dvar i q x
        | .neg => if !q then (if x.occursB i then .inv else .bi) else inferTyVar dvar i (!q) x
        | .inv => if x.occursB i then .inv else .bi
        | .bi => if x.occursB i then (if q then .pos else .neg) else .bi).join
      (inferTyVarArgs dvar i q T (j + 1) xs) := by
  conv => lhs; unfold inferTyVarArgs
  rfl

end Flick.Elab
