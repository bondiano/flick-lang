import Formal.TypeEquality

/-!
# Substitution of variables and regions on types, and what types reach

Shifting, lifting under a binder and instantiation; how `⌈·⌉` commutes with a
substitution, an atom becoming what its image reaches; how substitutions compose; and the congruence `Ty.Equiv`
under substitution. These are statements about type syntax only: what a row term stands
for is `Ty.asRow` (`Rows.lean`).

A substitution under a binder shifts what it puts there by a renaming (`Ty.ren`), so the
laws go through renamings first: `Ty.reach_ren` before `Ty.reach_substT`, `Ty.substT_ren` and
`Ty.ren_substT` before `Ty.substT_comp`. An arrow's stage is substituted into only if it is a
stage term, which a renaming keeps (`Ty.isStage_ren`) and a substitution of a stage term keeps
(`Ty.isStage_substT`).
-/

namespace Flick

/-- Instantiate the binders of a scheme with `ts`. -/
def instT (ts : List Ty) : Atom → Ty
  | .v i => if h : i < ts.length then ts[i] else .tvar (i - ts.length)
  | .r p => .rg p

@[simp] theorem Ty.reach_tvar (n : Nat) (a : Atom) : (Ty.tvar n).reach a ↔ a = .v n := Iff.rfl
@[simp] theorem Ty.reach_rg (p : Nat) (a : Atom) : (Ty.rg p).reach a ↔ a = .r p := Iff.rfl

@[simp] theorem Ty.reach_ident (b a : Atom) : (Ty.ident b).reach a ↔ a = b := by
  cases b <;> simp [Ty.ident]

/-- A variable below `k` is not an atom under `k` binders. -/
theorem Atom.up_ne {k i : Nat} {a : Atom} (hi : i < k) : a.up k ≠ .v i := by
  cases a <;> simp [Atom.up]; omega

theorem Atom.up_inj {k : Nat} {a b : Atom} (h : a.up k = b.up k) : a = b := by
  cases a <;> cases b <;> simp_all [Atom.up]

theorem Atom.ty_reach (b a : Atom) : b.ty.reach a ↔ a = b := by
  cases b <;> simp [Atom.ty]

/-- A renaming never makes a stage term: an atom is a variable or a region. -/
theorem Ty.isStage_ren (ξ : Atom → Atom) (t : Ty) : (t.ren ξ).isStage = t.isStage := by
  cases t with
  | tvar n => simp only [Ty.ren]; cases ξ (.v n) <;> rfl
  | rg p => simp only [Ty.ren]; cases ξ (.r p) <;> rfl
  | _ => rfl

/-- A substitution keeps a stage term one. -/
theorem Ty.isStage_substT {t : Ty} (σ : Atom → Ty) (h : t.isStage = true) :
    (t.substT σ).isStage = true := by
  cases t <;> first | rfl | simp [Ty.isStage] at h

theorem Ty.RowForm.isStage {r : Ty} (h : r.RowForm) : r.isStage = false := by
  cases h <;> rfl

/-- The atoms under `k` binders, and those of a lifted renaming. -/
theorem liftA_up (k : Nat) (ξ : Atom → Atom) (b : Atom) : liftA k ξ (b.up k) = (ξ b).up k := by
  cases b with
  | v i =>
    have : ¬ i + k < k := by omega
    simp [liftA, Atom.up, this]
  | r p => rfl

theorem liftA_cases (k : Nat) (ξ : Atom → Atom) (b : Atom) :
    (∃ i, i < k ∧ b = .v i ∧ liftA k ξ b = .v i) ∨ ∃ c : Atom, b = c.up k := by
  cases b with
  | v i =>
    by_cases hi : i < k
    · exact .inl ⟨i, hi, rfl, by simp [liftA, hi]⟩
    · exact .inr ⟨.v (i - k), by simp [Atom.up]; omega⟩
  | r p => exact .inr ⟨.r p, rfl⟩

/-- A renaming is a substitution by atoms. -/
theorem Ty.ren_eq (t : Ty) (ξ : Atom → Atom) : t.ren ξ = t.substT fun a => (ξ a).ty := by
  induction t generalizing ξ with
  | all k t ih =>
    simp only [Ty.ren, Ty.substT, ih]
    congr 2; funext a
    cases a with
    | v i =>
      by_cases hi : i < k
      · simp [liftA, liftT, hi, Atom.ty]
      · simp only [liftA, liftT, hi, if_false, Ty.up]
        cases ξ (.v (i - k)) <;> rfl
    | r p => simp only [liftA, liftT, Ty.up]; cases ξ (.r p) <;> rfl
  | _ => simp_all [Ty.ren, Ty.substT, Atom.ty]

theorem Ty.up_def (k : Nat) (t : Ty) : t.up k = t.substT (shiftV k) := by
  simp only [Ty.up, Ty.ren_eq]
  congr 1; funext a; cases a <;> rfl

theorem exists_or3 {p q r s : Atom → Prop} :
    ((∃ b, p b ∧ s b) ∨ (∃ b, q b ∧ s b) ∨ ∃ b, r b ∧ s b) ↔ ∃ b, (p b ∨ q b ∨ r b) ∧ s b := by
  constructor
  · rintro (⟨b, hb, h⟩ | ⟨b, hb, h⟩ | ⟨b, hb, h⟩)
    · exact ⟨b, .inl hb, h⟩
    · exact ⟨b, .inr (.inl hb), h⟩
    · exact ⟨b, .inr (.inr hb), h⟩
  · rintro ⟨b, hb | hb | hb, h⟩
    · exact .inl ⟨b, hb, h⟩
    · exact .inr (.inl ⟨b, hb, h⟩)
    · exact .inr (.inr ⟨b, hb, h⟩)

theorem exists_or2 {p q s : Atom → Prop} :
    ((∃ b, p b ∧ s b) ∨ ∃ b, q b ∧ s b) ↔ ∃ b, (p b ∨ q b) ∧ s b := by
  constructor
  · rintro (⟨b, hb, h⟩ | ⟨b, hb, h⟩)
    · exact ⟨b, .inl hb, h⟩
    · exact ⟨b, .inr hb, h⟩
  · rintro ⟨b, hb | hb, h⟩
    · exact .inl ⟨b, hb, h⟩
    · exact .inr ⟨b, hb, h⟩

/-- **⌈·⌉ commutes with a renaming.** -/
theorem Ty.reach_ren (ξ : Atom → Atom) (t : Ty) (a : Atom) :
    (t.ren ξ).reach a ↔ ∃ b, t.reach b ∧ (ξ b).ty.reach a := by
  induction t generalizing a ξ with
  | tvar n => simp [Ty.ren]
  | rg p => simp [Ty.ren]
  | caps C => simp [Ty.ren, Ty.reach, Caps.substT]
  | arrow x φ y C ihx _ ihy =>
    simp only [Ty.ren, Ty.reach, Caps.substT, ihx, ihy]
    exact exists_or3
  | ref t ih | rnm _ t ih | rless _ t ih | errors t ih | hnd t ih | owner t ih =>
    simp only [Ty.ren, Ty.reach, ih]
  | tapp f x ihf ihx | rcons _ f x ihf ihx | run f x ihf ihx =>
    simp only [Ty.ren, Ty.reach, ihf, ihx]
    exact exists_or2
  | eff r e t C _ _ ih =>
    simp only [Ty.ren, Ty.reach, Caps.substT, ih]
    exact exists_or2
  | base | adt | rnil | bare | dict | cnil | pure | rt | rtob => simp [Ty.ren, Ty.reach, Caps.none]
  | lazy r e t C _ ihe iht | lazyCell r e t C _ ihe iht =>
    simp only [Ty.ren, Ty.reach, Caps.substT, ihe, iht]
    exact exists_or3
  | all k t ih =>
    simp only [Ty.ren, Ty.reach, ih, Atom.ty_reach]
    constructor
    · rintro ⟨b, hb, h⟩
      rcases liftA_cases k ξ b with ⟨i, hi, rfl, he⟩ | ⟨c, rfl⟩
      · rw [he] at h; exact (Atom.up_ne hi h).elim
      · rw [liftA_up] at h; exact ⟨c, hb, Atom.up_inj h⟩
    · rintro ⟨b, hb, rfl⟩
      exact ⟨b.up k, hb, (liftA_up k ξ b).symm⟩

theorem Ty.reach_up_up (k : Nat) (t : Ty) (a : Atom) : (t.up k).reach (a.up k) ↔ t.reach a := by
  rw [Ty.up, Ty.reach_ren]
  constructor
  · rintro ⟨b, hb, h⟩
    have := (Atom.ty_reach _ _).mp h
    rwa [Atom.up_inj this]
  · exact fun h => ⟨a, h, (Atom.ty_reach _ _).mpr rfl⟩

/-- What a lifted substitution reaches under the binders. -/
theorem liftT_reach (k : Nat) (σ : Atom → Ty) (b a : Atom) :
    (liftT k σ b).reach (a.up k) ↔ ∃ c : Atom, b = c.up k ∧ (σ c).reach a := by
  rcases liftA_cases k id b with ⟨i, hi, rfl, -⟩ | ⟨c, rfl⟩
  · simp only [liftT, hi, if_true, Ty.reach_tvar]
    constructor
    · intro h; exact (Atom.up_ne hi h).elim
    · rintro ⟨c, h, -⟩; exact (Atom.up_ne hi h.symm).elim
  · have : liftT k σ (c.up k) = (σ c).up k := by
      cases c with
      | v i =>
        have : ¬ i + k < k := by omega
        simp [liftT, Atom.up, this]
      | r p => rfl
    rw [this, Ty.reach_up_up]
    constructor
    · exact fun h => ⟨c, rfl, h⟩
    · rintro ⟨c', h, h'⟩; rwa [Atom.up_inj h]

/-- **⌈·⌉ commutes with substitution**: `⌈τ[σ]⌉ = ⋃_{b ∈ ⌈τ⌉} ⌈σ b⌉`. -/
theorem Ty.reach_substT (σ : Atom → Ty) (t : Ty) (a : Atom) :
    (t.substT σ).reach a ↔ ∃ b, t.reach b ∧ (σ b).reach a := by
  induction t generalizing a σ with
  | tvar n => simp [Ty.substT]
  | rg p => simp [Ty.substT]
  | caps C => simp [Ty.substT, Ty.reach, Caps.substT]
  | arrow x φ y C ihx _ ihy =>
    simp only [Ty.substT, Ty.reach, Caps.substT, ihx, ihy]
    exact exists_or3
  | ref t ih | rnm _ t ih | rless _ t ih | errors t ih | hnd t ih | owner t ih =>
    simp only [Ty.substT, Ty.reach, ih]
  | tapp f x ihf ihx | rcons _ f x ihf ihx | run f x ihf ihx =>
    simp only [Ty.substT, Ty.reach, ihf, ihx]
    exact exists_or2
  | eff r e t C _ _ ih =>
    simp only [Ty.substT, Ty.reach, Caps.substT, ih]
    exact exists_or2
  | base | adt | rnil | bare | dict | cnil | pure | rt | rtob =>
    simp [Ty.substT, Ty.reach, Caps.none]
  | lazy r e t C _ ihe iht | lazyCell r e t C _ ihe iht =>
    simp only [Ty.substT, Ty.reach, Caps.substT, ihe, iht]
    exact exists_or3
  | all k t ih =>
    simp only [Ty.substT, Ty.reach, ih, liftT_reach]
    constructor
    · rintro ⟨b, hb, c, rfl, h⟩; exact ⟨c, hb, h⟩
    · rintro ⟨c, hb, h⟩; exact ⟨c.up k, hb, c, rfl, h⟩

theorem Caps.substT_reach (σ : Atom → Ty) (t : Ty) : t.reach.substT σ = (t.substT σ).reach := by
  funext a; exact propext (Ty.reach_substT σ t a).symm

@[simp] theorem Caps.none_substT (σ : Atom → Ty) : Caps.none.substT σ = Caps.none := by
  funext a; simp [Caps.substT, Caps.none]

theorem Caps.union_substT (C D : Caps) (σ : Atom → Ty) :
    Caps.substT σ (fun a => C a ∨ D a) = (fun a => C.substT σ a ∨ D.substT σ a) := by
  funext a; apply propext; simp only [Caps.substT]; grind

theorem Caps.substT_comp (C : Caps) (σ σ' : Atom → Ty) :
    (C.substT σ).substT σ' = C.substT fun a => (σ a).substT σ' := by
  funext a; apply propext
  simp only [Caps.substT, Ty.reach_substT]
  constructor
  · rintro ⟨b, ⟨c, hc, hb⟩, h⟩; exact ⟨c, hc, b, hb, h⟩
  · rintro ⟨c, hc, b, hb, h⟩; exact ⟨b, ⟨c, hc, hb⟩, h⟩

theorem Caps.substT_ident (C : Caps) : C.substT Ty.ident = C := by
  funext a; apply propext; simp [Caps.substT]

theorem Ty.substT_congr {t : Ty} {σ σ' : Atom → Ty} (h : ∀ a, σ a = σ' a) :
    t.substT σ = t.substT σ' := by
  rw [funext h]

theorem Atom.ty_substT (a : Atom) (σ : Atom → Ty) : a.ty.substT σ = σ a := by
  cases a <;> rfl

theorem liftT_up (k : Nat) (σ : Atom → Ty) (c : Atom) : liftT k σ (c.up k) = (σ c).up k := by
  cases c with
  | v i =>
    have : ¬ i + k < k := by omega
    simp [liftT, Atom.up, this]
  | r p => rfl

/-- Substituting after a renaming: the chain to `Ty.substT_comp` under a binder. -/
theorem Ty.substT_ren (t : Ty) (ξ : Atom → Atom) (σ : Atom → Ty) :
    (t.ren ξ).substT σ = t.substT fun a => σ (ξ a) := by
  induction t generalizing ξ σ with
  | tvar | rg => exact Atom.ty_substT _ _
  | all k t ih =>
    simp only [Ty.ren, Ty.substT, ih]
    congr 2; funext a
    rcases liftA_cases k ξ a with ⟨i, hi, rfl, he⟩ | ⟨c, rfl⟩
    · simp [he, liftT, hi]
    · rw [liftA_up, liftT_up, liftT_up]
  | arrow a φ b C iha ihφ ihb =>
    cases h : φ.isStage <;>
      simp [Ty.ren, Ty.substS, Caps.substT_comp, Atom.ty_substT, h, Ty.isStage_ren, iha, ihφ, ihb]
  | _ => simp_all [Ty.ren, Ty.substT, Caps.substT_comp, Atom.ty_substT]

theorem Ty.ren_ren (t : Ty) (ξ ξ' : Atom → Atom) : (t.ren ξ).ren ξ' = t.ren fun a => ξ' (ξ a) := by
  rw [Ty.ren_eq (t.ren ξ), Ty.substT_ren, Ty.ren_eq t]

theorem Ty.ren_substT (t : Ty) (σ : Atom → Ty) (ξ : Atom → Atom) :
    (t.substT σ).ren ξ = t.substT fun a => (σ a).ren ξ := by
  induction t generalizing σ ξ with
  | all k t ih =>
    simp only [Ty.ren, Ty.substT, ih]
    congr 2; funext a
    rcases liftA_cases k ξ a with ⟨i, hi, rfl, -⟩ | ⟨c, rfl⟩
    · simp [liftT, hi, Ty.ren, liftA, Atom.ty]
    · rw [liftT_up, liftT_up]
      simp only [Ty.up, Ty.ren_ren, liftA_up]
  | arrow a φ b C iha ihφ ihb =>
    cases h : φ.isStage <;>
      simp_all [Ty.substT, Caps.substT_comp, Ty.ren_eq, Ty.isStage_substT]
  | _ => simp_all [Ty.substT, Caps.substT_comp, Ty.ren_eq]

theorem Ty.up_comm (t : Ty) (k : Nat) (σ : Atom → Ty) :
    (t.up k).substT (liftT k σ) = (t.substT σ).up k := by
  simp only [Ty.up, Ty.substT_ren, Ty.ren_substT]
  exact Ty.substT_congr fun c => liftT_up k σ c

theorem liftT_comp (k : Nat) (σ σ' : Atom → Ty) (a : Atom) :
    (liftT k σ a).substT (liftT k σ') = liftT k (fun b => (σ b).substT σ') a := by
  cases a with
  | v i =>
    by_cases hi : i < k
    · simp [liftT, hi, Ty.substT]
    · simp only [liftT, hi, if_false]; exact Ty.up_comm _ k σ'
  | r p => simp only [liftT]; exact Ty.up_comm _ k σ'

theorem Ty.substT_comp (t : Ty) (σ σ' : Atom → Ty) :
    (t.substT σ).substT σ' = t.substT fun a => (σ a).substT σ' := by
  induction t generalizing σ σ' with
  | all k t ih =>
    simp only [Ty.substT, ih]
    congr 2; funext a
    exact liftT_comp k σ σ' a
  | arrow a φ b C iha ihφ ihb =>
    cases h : φ.isStage <;>
      simp [Ty.substS, Caps.substT_comp, h, Ty.isStage_substT, iha, ihφ, ihb]
  | _ => simp_all [Ty.substT, Caps.substT_comp]

theorem Ty.substS_comp (φ : Ty) (σ σ' : Atom → Ty) :
    (φ.substS σ).substS σ' = φ.substS fun a => (σ a).substT σ' := by
  cases h : φ.isStage <;> simp [Ty.substS, h, Ty.isStage_substT, Ty.substT_comp]

theorem liftT_ident (k : Nat) : liftT k Ty.ident = Ty.ident := by
  funext a
  cases a with
  | v i =>
    simp only [liftT]
    split
    · rfl
    · simp only [Ty.ident, Ty.up, Ty.ren, Atom.up, Atom.ty]; congr 1; omega
  | r p => rfl

theorem Ty.substT_ident (t : Ty) : t.substT Ty.ident = t := by
  induction t with
  | all k t ih => simp [Ty.substT, liftT_ident, ih]
  | arrow a φ b C iha ihφ ihb => simp [Ty.substS, Caps.substT_ident, iha, ihφ, ihb]
  | _ => simp_all [Ty.substT, Ty.ident, Caps.substT_ident]

theorem Ty.substS_ident (φ : Ty) : φ.substS Ty.ident = φ := by
  simp [Ty.substS, Ty.substT_ident]

/-! ## Congruence under substitution -/

theorem Ty.Equiv.reach {a b : Ty} (h : Ty.Equiv a b) : a.reach = b.reach := by
  induction h with
  | refl => rfl
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | run_comm r s => funext x; simp [Ty.reach, or_comm]
  | run_assoc r s t => funext x; simp [Ty.reach, or_assoc]
  | run_idem r _ => funext x; simp [Ty.reach]
  | run_nil r _ => funext x; simp [Ty.reach, Caps.none]
  | _ => simp_all [Ty.reach]

/-- Congruence keeps a stage term one, and anything else no stage. -/
theorem Ty.Equiv.isStage {a b : Ty} (h : Ty.Equiv a b) : a.isStage = b.isStage := by
  induction h with
  | refl => rfl
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | run_idem r hr | run_nil r hr | rless_none r hr => exact hr.isStage.symm
  | _ => rfl

theorem Ty.Equiv.substS {φ φ' : Ty} (h : Ty.Equiv φ φ') {σ : Atom → Ty}
    (ih : Ty.Equiv (φ.substT σ) (φ'.substT σ)) : Ty.Equiv (φ.substS σ) (φ'.substS σ) := by
  unfold Ty.substS; rw [h.isStage]; split
  · exact ih
  · exact h

theorem Ty.Equiv.substT {a b : Ty} (h : Ty.Equiv a b) (σ : Atom → Ty) :
    Ty.Equiv (a.substT σ) (b.substT σ) := by
  induction h generalizing σ with
  | refl => exact .refl
  | symm _ ih => exact .symm (ih σ)
  | trans _ _ ih₁ ih₂ => exact .trans (ih₁ σ) (ih₂ σ)
  | arrow _ hφ _ ih₁ ih₂ ih₃ => exact .arrow (ih₁ σ) (hφ.substS (ih₂ σ)) (ih₃ σ)
  | ref _ ih => exact .ref (ih σ)
  | tapp _ _ ih₁ ih₂ => exact .tapp (ih₁ σ) (ih₂ σ)
  | rcons _ _ ih₁ ih₂ => exact .rcons (ih₁ σ) (ih₂ σ)
  | eff _ _ _ ih₁ ih₂ ih₃ => exact .eff (ih₁ σ) (ih₂ σ) (ih₃ σ)
  | errors _ ih => exact .errors (ih σ)
  | lazy _ _ _ ih₁ ih₂ ih₃ => exact .lazy (ih₁ σ) (ih₂ σ) (ih₃ σ)
  | lazyCell _ _ _ ih₁ ih₂ ih₃ => exact .lazyCell (ih₁ σ) (ih₂ σ) (ih₃ σ)
  | hnd _ ih => exact .hnd (ih σ)
  | owner _ ih => exact .owner (ih σ)
  | dict _ ih => exact .dict (ih σ)
  | @all k _ _ _ ih => exact .all (ih (liftT k σ))
  | rnm _ ih => exact .rnm (ih σ)
  | run _ _ ih₁ ih₂ => exact .run (ih₁ σ) (ih₂ σ)
  | rless G _ ih => exact .rless G (ih σ)
  | rt _ _ ih₁ ih₂ => exact .rt (ih₁ σ) (ih₂ σ)
  | rtob _ _ _ ih₁ ih₂ ih₃ => exact .rtob (ih₁ σ) (ih₂ σ) (ih₃ σ)
  | run_comm r s => exact .run_comm (r.substT σ) (s.substT σ)
  | run_assoc r s t => exact .run_assoc (r.substT σ) (s.substT σ) (t.substT σ)
  | run_idem r hr => exact .run_idem (r.substT σ) (hr.substT σ)
  | run_nil r hr => exact .run_nil (r.substT σ) (hr.substT σ)
  | rless_nil G => exact .rless_nil G
  | rless_none r hr => exact .rless_none (r.substT σ) (hr.substT σ)
  | rless_run G r s => exact .rless_run G (r.substT σ) (s.substT σ)
  | rless_merge G H r => exact .rless_merge G H (r.substT σ)
  | rless_keep G N a h => exact .rless_keep G N (a.substT σ) h
  | rless_drop G N a h =>
    simpa only [Ty.substT, Caps.substT_reach] using Ty.Equiv.rless_drop G N (a.substT σ) h
  | caps_nil => simpa only [Ty.substT, Caps.none_substT] using Ty.Equiv.caps_nil
  | caps_union C D =>
    simpa only [Ty.substT, Caps.union_substT] using Ty.Equiv.caps_union (C.substT σ) (D.substT σ)
  | rless_caps G C => exact .rless_caps G (C.substT σ)

theorem Ty.Equiv.ren {a b : Ty} (h : Ty.Equiv a b) (ξ : Atom → Atom) :
    Ty.Equiv (a.ren ξ) (b.ren ξ) := by
  simpa only [Ty.ren_eq] using h.substT (fun a => (ξ a).ty)

theorem Ty.Equiv.up {a b : Ty} (h : Ty.Equiv a b) (k : Nat) :
    Ty.Equiv (a.up k) (b.up k) := h.ren (Atom.up k)

end Flick
