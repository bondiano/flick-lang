import Formal.Elab.CoreTr
import Formal.Elab.Basic

/-!
# λFlick⁺ as a `Target`: the refinement in the core

A derivation `Unify E θ` solves `E` in the source (`Unify.sound`) and gives the most general
unifier in the core too: every unifier `θ'` of the translated equations is `⟦θ⟧` followed by
`θ'` itself (`Unify.most`). A derivation `Clash E` leaves no unifier in the core
(`Clash.none`), when the type constructors have names apart.
-/

namespace Flick.Elab

theorem Ty.subst_bindV {x : Nat} {s t : Ty} (h : ¬ t.Occurs x) : t.subst (bindV x s) = t := by
  rw [Ty.subst_agree (σ' := .var) fun n hn => ?_, Ty.subst_id]
  simp only [bindV]
  split
  · subst_vars; exact absurd hn h
  · rfl

/-- **A derivation solves its equations.** -/
theorem Unify.sound {E θ} (h : Unify E θ) : ∀ p ∈ E, p.1.subst θ = p.2.subst θ := by
  induction h with
  | nil => intro p hp; cases hp
  | delete _ ih =>
    intro p hp
    rcases List.mem_cons.mp hp with rfl | hp
    · rfl
    · exact ih p hp
  | swap _ ih =>
    intro p hp
    rcases List.mem_cons.mp hp with rfl | hp
    · exact (ih _ (.head _)).symm
    · exact ih p (.tail _ hp)
  | app _ ih =>
    intro p hp
    rcases List.mem_cons.mp hp with rfl | hp
    · simp only [Ty.subst, ih _ (.head _), ih _ (.tail _ (.head _))]
    · exact ih p (.tail _ (.tail _ hp))
  | arr _ ih =>
    intro p hp
    rcases List.mem_cons.mp hp with rfl | hp
    · simp only [Ty.subst, ih _ (.head _), ih _ (.tail _ (.head _))]
    · exact ih p (.tail _ (.tail _ hp))
  | @bind x s E θ hx _ ih =>
    intro p hp
    have comp : ∀ t : Ty, t.subst (fun n => (bindV x s n).subst θ) = (t.subst (bindV x s)).subst θ :=
      fun t => (Ty.subst_comp t _ _).symm
    rcases List.mem_cons.mp hp with rfl | hp
    · show (bindV x s x).subst θ = s.subst (fun n => (bindV x s n).subst θ)
      rw [comp, Ty.subst_bindV hx]
      simp [bindV]
    · rw [comp, comp]
      exact ih _ (List.mem_map.mpr ⟨p, hp, rfl⟩)

variable (N : Names)

/-- `θ'` unifies the equations `E` in the core. -/
def CUnif (E : Eqs) (θ' : Atom → Flick.Ty) : Prop :=
  ∀ p ∈ E, (trT N p.1).substT θ' = (trT N p.2).substT θ'

variable {N}

theorem CUnif.cons {t u : Ty} {E : Eqs} {θ' : Atom → Flick.Ty} :
    CUnif N ((t, u) :: E) θ' ↔ (trT N t).substT θ' = (trT N u).substT θ' ∧ CUnif N E θ' := by
  simp [CUnif]

/-- A substitution that `θ'` fixes changes nothing under `θ'`. -/
theorem trT_subst_fix {σ : Nat → Ty} {θ' : Atom → Flick.Ty}
    (h : ∀ n, (trT N (σ n)).substT θ' = θ' (.v n)) (t : Ty) :
    (trT N (t.subst σ)).substT θ' = (trT N t).substT θ' := by
  rw [trT_subst, Flick.Ty.substT_comp]
  apply Flick.Ty.substT_congr
  intro a
  cases a with
  | v n => exact h n
  | r p => rfl

theorem CUnif.subst_bind {x : Nat} {s : Ty} {E : Eqs} {θ' : Atom → Flick.Ty}
    (hx : θ' (.v x) = (trT N s).substT θ') (h : CUnif N E θ') :
    CUnif N (Eqs.subst (bindV x s) E) θ' := by
  have fix : ∀ n, (trT N (bindV x s n)).substT θ' = θ' (.v n) := fun n => by
    simp only [bindV]
    split
    · subst_vars; exact hx.symm
    · rfl
  intro p hp
  obtain ⟨q, hq, rfl⟩ := List.mem_map.mp hp
  simp only [trT_subst_fix fix]
  exact h q hq

theorem CUnif.app {t u t' u' : Ty} {E : Eqs} {θ' : Atom → Flick.Ty}
    (h : CUnif N ((.app t u, .app t' u') :: E) θ') : CUnif N ((t, t') :: (u, u') :: E) θ' := by
  obtain ⟨h₁, h₂⟩ := CUnif.cons.mp h
  simp only [trT, Flick.Ty.substT, Flick.Ty.tapp.injEq] at h₁
  exact CUnif.cons.mpr ⟨h₁.1, CUnif.cons.mpr ⟨h₁.2, h₂⟩⟩

theorem CUnif.arr {φ : Stage} {q a b a' b' : Ty} {E : Eqs} {θ' : Atom → Flick.Ty}
    (h : CUnif N ((.arr φ a b q, .arr φ a' b' q) :: E) θ') :
    CUnif N ((a, a') :: (b, b') :: E) θ' := by
  obtain ⟨h₁, h₂⟩ := CUnif.cons.mp h
  rw [trT_arr, trT_arr] at h₁
  simp only [Flick.Ty.substT, Flick.Ty.arrow.injEq] at h₁
  exact CUnif.cons.mpr ⟨h₁.1, CUnif.cons.mpr ⟨h₁.2.2.1, h₂⟩⟩

/-- **The most general unifier in the core**: each unifier `θ'` of the translated equations
is `⟦θ⟧` followed by `θ'`. -/
theorem Unify.most {E θ} (h : Unify E θ) {θ' : Atom → Flick.Ty} (hu : CUnif N E θ') :
    ∀ n, (trT N (θ n)).substT θ' = θ' (.v n) := by
  induction h with
  | nil => intro n; rfl
  | delete _ ih => exact ih (CUnif.cons.mp hu).2
  | swap _ ih =>
    obtain ⟨h₁, h₂⟩ := CUnif.cons.mp hu
    exact ih (CUnif.cons.mpr ⟨h₁.symm, h₂⟩)
  | app _ ih => exact ih hu.app
  | arr _ ih => exact ih hu.arr
  | @bind x s E θ _ _ ih =>
    obtain ⟨h₁, h₂⟩ := CUnif.cons.mp hu
    have hx : θ' (.v x) = (trT N s).substT θ' := h₁
    have := ih (h₂.subst_bind hx)
    intro n
    rw [trT_subst_fix this]
    simp only [bindV]
    split
    · subst_vars; exact hx.symm
    · rfl

/-- `⟦θ⟧` unifies the translated equations. -/
theorem Unify.unifies {E θ} (h : Unify E θ) : CUnif N E (trσ N θ) := fun p hp => by
  rw [← trT_subst, ← trT_subst, h.sound p hp]

theorem trT_con_substT (c : Nat) (θ' : Atom → Flick.Ty) : (trT N (.con c)).substT θ' = trT N (.con c) := by
  simp only [trT]
  split
  · rfl
  · split <;> rfl

theorem trT_con_ne_tapp (c : Nat) (f a : Flick.Ty) : trT N (.con c) ≠ .tapp f a := by
  simp only [trT]
  split
  · simp
  · split <;> simp

theorem trT_con_ne_arrow (c : Nat) (a : Flick.Ty) (φ : Flick.Stage) (b : Flick.Ty) (C : Caps) :
    trT N (.con c) ≠ .arrow a φ b C := by
  simp only [trT]
  split
  · simp
  · split <;> simp

/-- Types with heads apart stay apart in the core. -/
theorem Ty.Apart.core (hinj : ∀ {c₁ c₂ : Nat}, c₁ ≠ c₂ → trT N (.con c₁) ≠ trT N (.con c₂))
    {t u : Ty} (h : t.Apart u) (θ' : Atom → Flick.Ty) :
    (trT N t).substT θ' ≠ (trT N u).substT θ' := by
  cases t <;> cases u <;> simp only [Ty.Apart] at h
  all_goals first
    | (rw [trT_con_substT, trT_con_substT]; exact hinj h)
    | (rw [trT_con_substT]; simp only [trT, Flick.Ty.substT]; exact trT_con_ne_tapp _ _ _)
    | (rw [trT_con_substT]; simp only [trT, Flick.Ty.substT]; exact (trT_con_ne_tapp _ _ _).symm)
    | (rw [trT_con_substT, trT_arr]; simp only [Flick.Ty.substT]; exact trT_con_ne_arrow _ _ _ _ _)
    | (rw [trT_con_substT, trT_arr]; simp only [Flick.Ty.substT]
       exact (trT_con_ne_arrow _ _ _ _ _).symm)
    | (rw [trT_arr]; simp [trT, Flick.Ty.substT])

/-- A variable inside the structure of a type is smaller than the type, under any substitution. -/
theorem Ty.OccS.size {x : Nat} {θ' : Atom → Flick.Ty} :
    ∀ {s : Ty}, s.OccS x → sizeOf (θ' (.v x)) ≤ sizeOf ((trT N s).substT θ') ∧
      (s ≠ .var x → sizeOf (θ' (.v x)) < sizeOf ((trT N s).substT θ'))
  | .var n, h => by
    simp only [Ty.OccS] at h; subst h
    exact ⟨Nat.le_refl _, fun h => absurd rfl h⟩
  | .app t u, h => by
    have key : sizeOf (θ' (.v x)) < sizeOf ((trT N (.app t u)).substT θ') := by
      simp only [trT, Flick.Ty.substT, Flick.Ty.tapp.sizeOf_spec]
      rcases h with h | h
      · have := (Ty.OccS.size (θ' := θ') (s := t) h).1; omega
      · have := (Ty.OccS.size (θ' := θ') (s := u) h).1; omega
    exact ⟨Nat.le_of_lt key, fun _ => key⟩
  | .arr φ a b q, h => by
    have key : sizeOf (θ' (.v x)) < sizeOf ((trT N (.arr φ a b q)).substT θ') := by
      rw [trT_arr]
      simp only [Flick.Ty.substT, Flick.Ty.arrow.sizeOf_spec]
      rcases h with h | h
      · have := (Ty.OccS.size (θ' := θ') (s := a) h).1; omega
      · have := (Ty.OccS.size (θ' := θ') (s := b) h).1; omega
    exact ⟨Nat.le_of_lt key, fun _ => key⟩
  | .con _, h | .cnil, h | .cun _ _, h | .rnm _ _, h | .run _ _, h => by simp [Ty.OccS] at h

/-- **No unifier in the core.** -/
theorem Clash.none (hinj : ∀ {c₁ c₂ : Nat}, c₁ ≠ c₂ → trT N (.con c₁) ≠ trT N (.con c₂))
    {E} (h : Clash E) : ∀ θ', ¬ CUnif N E θ' := by
  induction h with
  | apart ha => intro θ' hu; exact ha.core hinj θ' (CUnif.cons.mp hu).1
  | @occurs x s E hne ho =>
    intro θ' hu
    have h₁ : θ' (.v x) = (trT N s).substT θ' := (CUnif.cons.mp hu).1
    have := (Ty.OccS.size (N := N) (θ' := θ') ho).2 hne
    rw [← h₁] at this
    exact Nat.lt_irrefl _ this
  | delete _ ih => intro θ' hu; exact ih θ' (CUnif.cons.mp hu).2
  | swap _ ih =>
    intro θ' hu
    obtain ⟨h₁, h₂⟩ := CUnif.cons.mp hu
    exact ih θ' (CUnif.cons.mpr ⟨h₁.symm, h₂⟩)
  | app _ ih => intro θ' hu; exact ih θ' hu.app
  | arr _ ih => intro θ' hu; exact ih θ' hu.arr
  | bind _ _ ih =>
    intro θ' hu
    obtain ⟨h₁, h₂⟩ := CUnif.cons.mp hu
    exact ih θ' (h₂.subst_bind h₁)

end Flick.Elab
