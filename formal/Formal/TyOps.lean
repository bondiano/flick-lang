import Formal.Syntax

/-!
# Type variables: shifting, lifting under a scheme, instantiation
-/

namespace Flick

/-- The renaming `i ↦ i + k` as a substitution. -/
def shiftV (k : Nat) (i : Nat) : Ty := .tvar (i + k)

/-- Shift the free type variables by `k`. -/
def Ty.up (k : Nat) (t : Ty) : Ty := t.substT (shiftV k)

/-- A substitution under the `k` binders of a scheme. -/
def liftT (k : Nat) (ρ : Nat → Ty) (i : Nat) : Ty :=
  if i < k then .tvar i else (ρ (i - k)).up k

/-- Instantiate the binders of a scheme with `ts`. -/
def instT (ts : List Ty) (i : Nat) : Ty :=
  if h : i < ts.length then ts[i] else .tvar (i - ts.length)

def Scheme.substT (ρ : Nat → Ty) (s : Scheme) : Scheme := ⟨s.k, s.t.substT (liftT s.k ρ)⟩

def Scheme.up (k : Nat) (s : Scheme) : Scheme := s.substT (shiftV k)

/-- `σ[ᾱ := τ̄]`. -/
def Scheme.inst (s : Scheme) (ts : List Ty) : Ty := s.t.substT (instT ts)

theorem Ty.substT_comp (t : Ty) (ρ ρ' : Nat → Ty) :
    (t.substT ρ).substT ρ' = t.substT fun i => (ρ i).substT ρ' := by
  induction t <;> simp_all [Ty.substT]

theorem Ty.substT_tvar (t : Ty) : t.substT .tvar = t := by
  induction t <;> simp_all [Ty.substT]

theorem Ty.substT_congr {t : Ty} {ρ ρ' : Nat → Ty} (h : ∀ i, ρ i = ρ' i) :
    t.substT ρ = t.substT ρ' := by
  rw [funext h]

@[simp] theorem Ty.up_zero (t : Ty) : t.up 0 = t := by
  simp only [Ty.up]; exact t.substT_tvar

@[simp] theorem List.map_up_zero (Ψ : List Ty) : Ψ.map (Ty.up 0) = Ψ := by
  induction Ψ <;> simp_all

@[simp] theorem liftT_zero (ρ : Nat → Ty) : liftT 0 ρ = ρ := by
  funext i; simp [liftT]

theorem liftT_tvar (k : Nat) : liftT k .tvar = .tvar := by
  funext i
  unfold liftT
  split
  · rfl
  · simp [Ty.up, Ty.substT, shiftV]; omega

theorem liftT_comp (k : Nat) (ρ ρ' : Nat → Ty) (i : Nat) :
    (liftT k ρ i).substT (liftT k ρ') = liftT k (fun j => (ρ j).substT ρ') i := by
  unfold liftT
  split
  · simp [Ty.substT, *]
  · simp only [Ty.up, Ty.substT_comp]
    apply Ty.substT_congr
    intro j
    have : ¬ j + k < k := by omega
    simp [shiftV, Ty.substT, this]

theorem Ty.up_substT (t : Ty) (k : Nat) (ts : List Ty) (h : ts.length = k) :
    (t.up k).substT (instT ts) = t := by
  simp only [Ty.up, Ty.substT_comp]
  conv => rhs; rw [← t.substT_tvar]
  apply Ty.substT_congr
  intro i
  have : ¬ i + k < k := by omega
  simp [shiftV, Ty.substT, instT, this, h]

theorem Ty.inst_substT (t : Ty) (ρ : Nat → Ty) (ts : List Ty) :
    (t.substT (liftT ts.length ρ)).substT (instT (ts.map (Ty.substT ρ))) =
      (t.substT (instT ts)).substT ρ := by
  simp only [Ty.substT_comp]
  apply Ty.substT_congr
  intro i
  unfold liftT instT
  by_cases hi : i < ts.length
  · simp [hi, Ty.substT]
  · have hi' : ¬ i < (ts.map (Ty.substT ρ)).length := by simpa using hi
    rw [if_neg hi, dif_neg hi]
    simp only [Ty.substT]
    exact Ty.up_substT _ _ _ (by simp)

theorem Ty.up_comm (t : Ty) (k : Nat) (ρ : Nat → Ty) :
    (t.up k).substT (liftT k ρ) = (t.substT ρ).up k := by
  simp only [Ty.up, Ty.substT_comp]
  apply Ty.substT_congr
  intro i
  have : ¬ i + k < k := by omega
  simp [shiftV, Ty.substT, liftT, this, Ty.up]

theorem Ty.up_up (t : Ty) (a b : Nat) : (t.up a).up b = t.up (a + b) := by
  simp only [Ty.up, Ty.substT_comp]
  apply Ty.substT_congr
  intro i
  simp [shiftV, Ty.substT, Nat.add_assoc]

theorem Scheme.substT_comp (s : Scheme) (ρ ρ' : Nat → Ty) :
    (s.substT ρ).substT ρ' = s.substT fun i => (ρ i).substT ρ' := by
  simp only [Scheme.substT, Ty.substT_comp, Scheme.mk.injEq, true_and]
  apply Ty.substT_congr
  intro i; exact liftT_comp _ _ _ _

theorem Scheme.substT_tvar (s : Scheme) : s.substT .tvar = s := by
  simp [Scheme.substT, liftT_tvar, Ty.substT_tvar]

theorem Scheme.substT_congr {s : Scheme} {ρ ρ' : Nat → Ty} (h : ∀ i, ρ i = ρ' i) :
    s.substT ρ = s.substT ρ' := by
  rw [funext h]

@[simp] theorem Scheme.up_zero (s : Scheme) : s.up 0 = s := by
  have : shiftV 0 = .tvar := by funext i; simp [shiftV]
  simp only [Scheme.up, this]; exact s.substT_tvar

theorem Scheme.up_comm (s : Scheme) (k : Nat) (ρ : Nat → Ty) :
    (s.up k).substT (liftT k ρ) = (s.substT ρ).up k := by
  simp only [Scheme.up, Scheme.substT_comp]
  apply Scheme.substT_congr
  intro i
  have : ¬ i + k < k := by omega
  simp [shiftV, Ty.substT, liftT, Ty.up, this]

theorem Scheme.up_substT (s : Scheme) (k : Nat) (ts : List Ty) (h : ts.length = k) :
    (s.up k).substT (instT ts) = s := by
  simp only [Scheme.up, Scheme.substT_comp]
  conv => rhs; rw [← s.substT_tvar]
  apply Scheme.substT_congr
  intro i
  have : ¬ i + k < k := by omega
  simp [shiftV, Ty.substT, instT, h, this]

@[simp] theorem List.map_sup_zero (Γ : List Scheme) : Γ.map (Scheme.up 0) = Γ := by
  induction Γ <;> simp_all

@[simp] theorem Scheme.mono_k (t : Ty) : (Scheme.mono t).k = 0 := rfl
@[simp] theorem Scheme.mono_t (t : Ty) : (Scheme.mono t).t = t := rfl

@[simp] theorem Scheme.mono_substT (t : Ty) (ρ : Nat → Ty) :
    (Scheme.mono t).substT ρ = Scheme.mono (t.substT ρ) := by
  simp [Scheme.substT, Scheme.mono]

@[simp] theorem Scheme.inst_mono (t : Ty) : (Scheme.mono t).inst [] = t := by
  simp only [Scheme.inst, Scheme.mono]
  conv => rhs; rw [← t.substT_tvar]
  apply Ty.substT_congr; intro i; simp [instT]

theorem Scheme.inst_substT (s : Scheme) (ρ : Nat → Ty) (ts : List Ty) (h : ts.length = s.k) :
    (s.substT ρ).inst (ts.map (Ty.substT ρ)) = (s.inst ts).substT ρ := by
  simp only [Scheme.substT, Scheme.inst]
  rw [← h]; exact Ty.inst_substT _ _ _

theorem Ty.arrows_substT (ts : List Ty) (r : Ty) (ρ : Nat → Ty) :
    (Ty.arrows ts r).substT ρ = Ty.arrows (ts.map (Ty.substT ρ)) (r.substT ρ) := by
  induction ts <;> simp_all [Ty.arrows, Ty.substT]

/-- A record type: a spine of fields ending in `rnil`. -/
def Ty.IsRec : Ty → Prop
  | .rnil => True
  | .rcons _ _ r => r.IsRec
  | _ => False

theorem Ty.labels_substT {t : Ty} (ρ : Nat → Ty) (h : t.IsRec) :
    (t.substT ρ).labels = t.labels := by
  induction t <;> simp_all [Ty.labels, Ty.substT, Ty.IsRec]

theorem Ty.IsRec.substT {t : Ty} (ρ : Nat → Ty) (h : t.IsRec) : (t.substT ρ).IsRec := by
  induction t <;> simp_all [Ty.substT, Ty.IsRec]

theorem Ty.field_substT {R : Ty} {l t} (ρ : Nat → Ty) (h : R.field l = some t) :
    (R.substT ρ).field l = some (t.substT ρ) := by
  induction R <;> simp_all [Ty.field, Ty.substT]
  split <;> simp_all

theorem Sig.ctor_ty_substT (S : Sig) {K ts T} (ρ : Nat → Ty) (h : S.ctor K = some (ts, T)) :
    (Ty.arrows ts (.adt T)).substT ρ = Ty.arrows ts (.adt T) := by
  rw [Ty.arrows_substT, S.ctor_closed K ts T ρ h]; rfl

theorem Const.ty_substT (S : Sig) (c : Const) (t : Ty) (ρ : Nat → Ty) :
    (c.ty S t).substT ρ = c.ty S (t.substT ρ) := by
  cases c <;> simp [Const.ty, Ty.substT, S.impl_closed]

end Flick
