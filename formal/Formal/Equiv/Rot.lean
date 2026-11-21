import Formal.Equiv.PureOpen

/-!
# Contextual equivalence: loop binder rotation `rot` and term size
-/

namespace Flick.Equiv

/-- Rotation of the three binders of a hoisted loop: `l` (`0` inside `let l = lazy (e↑2) in b`)
moves outside `fix` (which binds argument `0` and self `1`). -/
def rot : Nat → Nat
  | 0 => 2
  | 1 => 0
  | 2 => 1
  | i + 3 => i + 3

theorem liftR2_up2 (ξ : Nat → Nat) : (liftR 2 ξ) ∘ (· + 2) = (· + 2) ∘ ξ := by
  funext i; simp only [Function.comp, liftR]; rw [if_neg (by omega), Nat.add_sub_cancel]

theorem rename_up2 (e : Tm) (ξ : Nat → Nat) :
    (e.rename (· + 2)).rename (liftR 2 ξ) = (e.rename ξ).rename (· + 2) := by
  rw [Tm.rename_rename, Tm.rename_rename, liftR2_up2]

theorem subst_up2 (e : Tm) (σ : Nat → Tm) :
    (e.rename (· + 2)).subst (liftS 2 σ) = (e.subst σ).rename (· + 2) := by
  rw [Tm.subst_rename, Tm.rename_subst]
  congr 1

theorem subst_shift2 (e v f : Tm) : (e.rename (· + 2)).subst (inst [v, f]) = e := by
  rw [Tm.subst_rename]
  refine Tm.subst_fix _ _ fun i _ => ?_
  simp only [Function.comp, inst, List.length_cons, List.length_nil]
  rw [dif_neg (by omega), Nat.add_sub_cancel]

theorem rot_rename (b : Tm) (ξ : Nat → Nat) :
    (b.rename rot).rename (liftR 2 (liftR 1 ξ)) = (b.rename (liftR 1 (liftR 2 ξ))).rename rot := by
  rw [Tm.rename_rename, Tm.rename_rename]
  congr 1; funext k
  match k with
  | 0 => rfl
  | 1 => rfl
  | 2 => rfl
  | i + 3 =>
    have hL : liftR 2 (liftR 1 ξ) (rot (i + 3)) = ξ i + 3 := by
      simp only [rot, liftR]
      rw [if_neg (by omega), show i + 3 - 2 = i + 1 by omega, if_neg (by omega),
        show i + 1 - 1 = i by omega]
    have hR : rot (liftR 1 (liftR 2 ξ) (i + 3)) = ξ i + 3 := by
      simp only [liftR]
      rw [if_neg (by omega), show i + 3 - 1 = i + 2 by omega, if_neg (by omega),
        show i + 2 - 2 = i by omega]
      rfl
    exact hL.trans hR.symm

theorem rot_subst (b : Tm) (σ : Nat → Tm) :
    (b.rename rot).subst (liftS 2 (liftS 1 σ)) = (b.subst (liftS 1 (liftS 2 σ))).rename rot := by
  rw [Tm.subst_rename, Tm.rename_subst]
  congr 1; funext k
  match k with
  | 0 => simp [Function.comp, rot, liftS, Tm.rename]
  | 1 => simp [Function.comp, rot, liftS, Tm.rename]
  | 2 => simp [Function.comp, rot, liftS, Tm.rename]
  | i + 3 =>
    have hL : liftS 2 (liftS 1 σ) (rot (i + 3)) = ((σ i).rename (· + 1)).rename (· + 2) := by
      simp only [rot, liftS]
      rw [if_neg (by omega), show i + 3 - 2 = i + 1 by omega, if_neg (by omega),
        show i + 1 - 1 = i by omega]
    have hR : (liftS 1 (liftS 2 σ) (i + 3)).rename rot = (((σ i).rename (· + 2)).rename (· + 1)).rename rot := by
      simp only [liftS]
      rw [if_neg (by omega), show i + 3 - 1 = i + 2 by omega, if_neg (by omega),
        show i + 2 - 2 = i by omega]
    simp only [Function.comp]
    rw [hL, hR, Tm.rename_rename, Tm.rename_rename, Tm.rename_rename]
    congr 1

theorem subst_rot_loc (b : Tm) (l : Nat) :
    (b.rename rot).subst (liftS 2 (inst [.loc l])) = b.subst (inst [.loc l]) := by
  rw [Tm.subst_rename]
  congr 1; funext k
  match k with
  | 0 => simp [Function.comp, rot, liftS, inst, Tm.rename]
  | 1 => simp [Function.comp, rot, liftS, inst]
  | 2 => simp [Function.comp, rot, liftS, inst]
  | i + 3 =>
    have hL : liftS 2 (inst [.loc l]) (rot (i + 3)) = .var (i + 2) := by
      simp only [rot, liftS]
      rw [if_neg (by omega), show i + 3 - 2 = i + 1 by omega]
      simp only [inst, List.length_cons, List.length_nil]
      rw [dif_neg (by omega), show i + 1 - 1 = i by omega]
      rfl
    have hR : inst [.loc l] (i + 3) = .var (i + 2) := by
      simp only [inst, List.length_cons, List.length_nil]
      rw [dif_neg (by omega), show i + 3 - 1 = i + 2 by omega]
    exact hL.trans hR.symm

theorem subst_loc_rename (c : Tm) (l : Nat) (ξ : Nat → Nat) :
    (c.subst (inst [.loc l])).rename ξ = (c.rename (liftR 1 ξ)).subst (inst [.loc l]) := by
  rw [Tm.rename_subst, Tm.subst_rename]
  congr 1; funext k
  cases k with
  | zero => simp [Function.comp, inst, liftR, Tm.rename]
  | succ i =>
    have hL : (inst [.loc l] (i + 1)).rename ξ = .var (ξ i) := by
      simp only [inst, List.length_cons, List.length_nil]
      rw [dif_neg (by omega), show i + 1 - 1 = i by omega]
      rfl
    have hR : inst [.loc l] (liftR 1 ξ (i + 1)) = .var (ξ i) := by
      have hl : liftR 1 ξ (i + 1) = ξ i + 1 := by
        simp only [liftR]; rw [if_neg (by omega), show i + 1 - 1 = i by omega]
      rw [hl]
      simp only [inst, List.length_cons, List.length_nil]
      rw [dif_neg (by omega), show ξ i + 1 - 1 = ξ i by omega]
    exact hL.trans hR.symm

theorem subst_loc_rename2 (b : Tm) (l : Nat) (ξ : Nat → Nat) :
    (b.subst (inst [.loc l])).rename (liftR 2 ξ) =
      (b.rename (liftR 1 (liftR 2 ξ))).subst (inst [.loc l]) :=
  subst_loc_rename b l (liftR 2 ξ)

theorem subst_loc_subst (c : Tm) (l : Nat) (σ : Nat → Tm) :
    (c.subst (inst [.loc l])).subst σ = (c.subst (liftS 1 σ)).subst (inst [.loc l]) := by
  rw [Tm.subst_subst, Tm.subst_subst]
  congr 1; funext k
  cases k with
  | zero => simp [inst, liftS, Tm.subst]
  | succ i =>
    simp only [inst, liftS, List.length_cons, List.length_nil]
    rw [dif_neg (by omega), if_neg (by omega), show i + 1 - 1 = i by omega]
    simp only [Tm.subst]
    rw [Tm.subst_rename]
    exact (Tm.subst_fix _ _ fun j _ => by
      simp only [Function.comp, inst, List.length_cons, List.length_nil]
      rw [dif_neg (by omega), show j + 1 - 1 = j by omega]).symm

theorem subst_loc_subst2 (b : Tm) (l : Nat) (σ : Nat → Tm) :
    (b.subst (inst [.loc l])).subst (liftS 2 σ) =
      (b.subst (liftS 1 (liftS 2 σ))).subst (inst [.loc l]) :=
  subst_loc_subst b l (liftS 2 σ)

theorem subst_loc_inst2 (b : Tm) (l : Nat) (v f : Tm) :
    (b.subst (inst [.loc l])).subst (inst [v, f]) =
      (b.subst (liftS 1 (inst [v, f]))).subst (inst [.loc l]) :=
  subst_loc_subst b l (inst [v, f])

/-! ## Structural term size preserved by renaming and location substitution -/

/-- Structural node count of a term: every leaf has size 1, so substituting a location keeps it. -/
def _root_.Flick.Tm.sz : Tm → Nat
  | .var _ | .lit _ | .loc _ | .const _ | .ctor _ | .rnil | .anil | .todo
  | .cap _ | .own _ | .glob _ | .kont _ _ | .hole _ => 1
  | .lam b | .fix b | .proj b _ | .task b | .await b | .lazy b | .force b
  | .raise b | .scope b | .inside _ b | .drec _ b | .hdone _ b | .unw _ _ b
  | .settle _ b => b.sz + 1
  | .app f a | .let_ f a | .letrec f a | .rcons _ f a | .update f _ a
  | .match_ f a | .acons _ f a | .try_ f a | .handle f a
  | .hres _ _ f a | .hkeep _ _ _ f a => f.sz + a.sz + 1
  | .aguard _ g b r | .hdl _ g b r | .hwith _ _ _ g b r | .hkarm _ _ _ g b r =>
    g.sz + b.sz + r.sz + 1
  | .hctl _ _ _ _ _ al r a c => al.sz + r.sz + a.sz + c.sz + 1

theorem sz_rename : ∀ (e : Tm) (ξ : Nat → Nat), (e.rename ξ).sz = e.sz := by
  intro e
  induction e <;> intro ξ <;> simp only [Tm.rename, Tm.sz] <;> simp_all

theorem sz_subst_atom : ∀ (e : Tm) (σ : Nat → Tm), (∀ i, (σ i).sz = 1) → (e.subst σ).sz = e.sz := by
  have lift : ∀ {n : Nat} {σ : Nat → Tm}, (∀ i, (σ i).sz = 1) → ∀ i, (liftS n σ i).sz = 1 := by
    intro n σ h i; unfold liftS; split
    · rfl
    · rw [sz_rename]; exact h _
  intro e
  induction e <;> intro σ h <;> simp only [Tm.subst, Tm.sz] <;>
    simp_all [lift h]

theorem sz_subst_loc (e : Tm) (l : Nat) : (e.subst (inst [.loc l])).sz = e.sz :=
  sz_subst_atom e (inst [.loc l]) fun i => by cases i <;> rfl

end Flick.Equiv
