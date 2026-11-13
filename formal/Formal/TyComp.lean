import Formal.RowLess

/-!
# Substitution of variables and regions: shifting, lifting under a binder, instantiation;
how `⌈·⌉` commutes with a substitution (regions.md §2.1), and how substitutions compose

A substitution under a binder shifts what it puts there by a renaming (`Ty.ren`), so the
laws go through renamings first: `Ty.reach_ren` before `Ty.reach_substT`, `Ty.substT_ren` and
`Ty.ren_substT` before `Ty.substT_comp`.
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

theorem Atom.ty_asRow (b : Atom) (x : Elem) : b.ty.asRow x ↔ x = .rv b fun _ => False := by
  cases b <;> simp [Atom.ty, Ty.asRow]

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

/-- **⌈·⌉ commutes with a renaming.** -/
theorem Ty.reach_ren (ξ : Atom → Atom) (t : Ty) (a : Atom) :
    (t.ren ξ).reach a ↔ ∃ b, t.reach b ∧ (ξ b).ty.reach a := by
  induction t generalizing a ξ with
  | tvar n => simp [Ty.ren]
  | rg p => simp [Ty.ren]
  | caps C => simp [Ty.ren, Ty.reach, Caps.substT]
  | row r =>
    simp only [Ty.ren, Ty.reach]
    exact Row.substT_reach _ _ a
  | arrow x φ y C ihx ihy =>
    simp only [Ty.ren, Ty.reach, Caps.substT, ihx, ihy]
    constructor
    · rintro (⟨b, hb, h⟩ | ⟨b, hb, h⟩ | ⟨b, hb, h⟩)
      · exact ⟨b, .inl hb, h⟩
      · exact ⟨b, .inr (.inl hb), h⟩
      · exact ⟨b, .inr (.inr hb), h⟩
    · rintro ⟨b, hb | hb | hb, h⟩
      · exact .inl ⟨b, hb, h⟩
      · exact .inr (.inl ⟨b, hb, h⟩)
      · exact .inr (.inr ⟨b, hb, h⟩)
  | ref t ih | rnm _ t ih | rless _ t ih => simp only [Ty.ren, Ty.reach, ih]
  | tapp f x ihf ihx | rcons _ f x ihf ihx | run f x ihf ihx =>
    simp only [Ty.ren, Ty.reach, ihf, ihx]
    constructor
    · rintro (⟨b, hb, h⟩ | ⟨b, hb, h⟩)
      · exact ⟨b, .inl hb, h⟩
      · exact ⟨b, .inr hb, h⟩
    · rintro ⟨b, hb | hb, h⟩
      · exact .inl ⟨b, hb, h⟩
      · exact .inr ⟨b, hb, h⟩
  | eff r e t C ih =>
    simp only [Ty.ren, Ty.reach, Caps.substT, ih]
    constructor
    · rintro (⟨b, hb, h⟩ | ⟨b, hb, h⟩)
      · exact ⟨b, .inl hb, h⟩
      · exact ⟨b, .inr hb, h⟩
    · rintro ⟨b, hb | hb, h⟩
      · exact .inl ⟨b, hb, h⟩
      · exact .inr ⟨b, hb, h⟩
  | errors e =>
    simp only [Ty.ren, Ty.reach]
    exact Row.substT_reach _ _ a
  | hnd s ih | owner s ih => simp only [Ty.ren, Ty.reach, ih]
  | base | adt | rnil | bare | dict | cnil | pure | rt => simp [Ty.ren, Ty.reach, Caps.none]
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
  | row r =>
    simp only [Ty.substT, Ty.reach]
    exact Row.substT_reach σ _ a
  | arrow x φ y C ihx ihy =>
    simp only [Ty.substT, Ty.reach, Caps.substT, ihx, ihy]
    constructor
    · rintro (⟨b, hb, h⟩ | ⟨b, hb, h⟩ | ⟨b, hb, h⟩)
      · exact ⟨b, .inl hb, h⟩
      · exact ⟨b, .inr (.inl hb), h⟩
      · exact ⟨b, .inr (.inr hb), h⟩
    · rintro ⟨b, hb | hb | hb, h⟩
      · exact .inl ⟨b, hb, h⟩
      · exact .inr (.inl ⟨b, hb, h⟩)
      · exact .inr (.inr ⟨b, hb, h⟩)
  | ref t ih | rnm _ t ih | rless _ t ih => simp only [Ty.substT, Ty.reach, ih]
  | tapp f x ihf ihx | rcons _ f x ihf ihx | run f x ihf ihx =>
    simp only [Ty.substT, Ty.reach, ihf, ihx]
    constructor
    · rintro (⟨b, hb, h⟩ | ⟨b, hb, h⟩)
      · exact ⟨b, .inl hb, h⟩
      · exact ⟨b, .inr hb, h⟩
    · rintro ⟨b, hb | hb, h⟩
      · exact .inl ⟨b, hb, h⟩
      · exact .inr ⟨b, hb, h⟩
  | eff r e t C ih =>
    simp only [Ty.substT, Ty.reach, Caps.substT, ih]
    constructor
    · rintro (⟨b, hb, h⟩ | ⟨b, hb, h⟩)
      · exact ⟨b, .inl hb, h⟩
      · exact ⟨b, .inr hb, h⟩
    · rintro ⟨b, hb | hb, h⟩
      · exact .inl ⟨b, hb, h⟩
      · exact .inr ⟨b, hb, h⟩
  | errors e =>
    simp only [Ty.substT, Ty.reach]
    exact Row.substT_reach σ _ a
  | hnd s ih | owner s ih => simp only [Ty.substT, Ty.reach, ih]
  | base | adt | rnil | bare | dict | cnil | pure | rt => simp [Ty.substT, Ty.reach, Caps.none]
  | all k t ih =>
    simp only [Ty.substT, Ty.reach, ih, liftT_reach]
    constructor
    · rintro ⟨b, hb, c, rfl, h⟩; exact ⟨c, hb, h⟩
    · rintro ⟨c, hb, h⟩; exact ⟨c.up k, hb, c, rfl, h⟩

theorem Caps.substT_reach (σ : Atom → Ty) (t : Ty) : t.reach.substT σ = (t.substT σ).reach := by
  funext a; exact propext (Ty.reach_substT σ t a).symm

theorem Row.ofCaps_substT (C : Caps) (σ : Atom → Ty) :
    (Row.ofCaps C).substT σ = Row.ofCaps (C.substT σ) := by
  funext x; cases x <;> simp [Row.substT, Row.ofCaps, Caps.substT]

theorem Ty.asRow_tvar (n : Nat) : (Ty.tvar n).asRow = fun x => x = .rv (.v n) fun _ => False :=
  rfl
theorem Ty.asRow_rg (p : Nat) : (Ty.rg p).asRow = fun x => x = .rv (.r p) fun _ => False := rfl
theorem Ty.asRow_row (r : Row) : (Ty.row r).asRow = r.plain := rfl

@[simp] theorem Row.none_substT (σ : Atom → Ty) : Row.none.substT σ = Row.none := by
  funext x; cases x <;> simp [Row.substT, Row.none]

@[simp] theorem Row.single_substT (σ : Atom → Ty) (N : Name) :
    (Row.single N).substT σ = Row.single N := by
  funext x; cases x <;> simp [Row.substT, Row.single]

@[simp] theorem Row.union_substT (σ : Atom → Ty) (r r' : Row) :
    (Row.union r r').substT σ = Row.union (r.substT σ) (r'.substT σ) := by
  funext x; apply propext
  cases x <;> simp only [Row.substT, Row.union] <;> grind

theorem Row.less_substT (G : Name → Prop) (r : Row) (σ : Atom → Ty) :
    (r.less G).substT σ = (r.substT σ).less G := by
  funext x; apply propext
  cases x with
  | nm N | atm a => simp only [Row.less, Row.substT]; grind
  | ob N a | obv v H a => simp [Row.less, Row.substT]
  | rv c H =>
    constructor
    · rintro ⟨b, K, ⟨K', hb, rfl⟩, H', hc, rfl⟩
      exact ⟨fun n => K' n ∨ H' n, ⟨b, K', hb, H', hc, rfl⟩, by
        funext n; apply propext; exact or_assoc⟩
    · rintro ⟨K, ⟨b, K', hb, H', hc, rfl⟩, rfl⟩
      exact ⟨b, fun n => G n ∨ K' n, ⟨K', hb, rfl⟩, H', hc, by
        funext n; apply propext; exact or_assoc.symm⟩

/-- What a type stands for in a row commutes with substitution. -/
theorem Ty.asRow_substT (σ : Atom → Ty) (t : Ty) : (t.substT σ).asRow = t.asRow.substT σ := by
  have hc : ∀ t' : Ty, t'.asRow = Row.ofCaps t'.reach → (t'.substT σ).asRow = Row.ofCaps (t'.substT σ).reach →
      (t'.substT σ).asRow = t'.asRow.substT σ := by
    intro t' h₁ h₂
    rw [h₁, h₂, Row.ofCaps_substT]; congr 1; funext a; apply propext; exact Ty.reach_substT σ t' a
  induction t with
  | tvar n => exact (Row.var_substT σ (.v n)).symm
  | rg p => exact (Row.var_substT σ (.r p)).symm
  | row r =>
    funext x; apply propext; cases x <;> simp [Ty.substT, Ty.asRow_row, Row.substT, Row.plain]
  | cnil => simp [Ty.substT, Ty.asRow]
  | rnm N args ih =>
    simp only [Ty.substT, Ty.asRow, Row.union_substT, Row.single_substT, Row.ofCaps_substT,
      Caps.substT_reach]
  | run left right ihleft ihright =>
    simp only [Ty.substT, Ty.asRow, ihleft, ihright, Row.union_substT]
  | rless gone r ih =>
    simp only [Ty.substT, Ty.asRow, ih, Row.less_substT]
  | _ => exact hc _ rfl rfl

/-- A substitution that brings no name into a row: the instance of each row variable has
no service and no error. -/
def NoNames (σ : Atom → Ty) : Prop := ∀ b N, ¬ (σ b).asRow (.nm N)

theorem Row.substT_nm {σ : Atom → Ty} (h : NoNames σ) (r : Row) (N : Name) :
    r.substT σ (.nm N) ↔ r (.nm N) :=
  ⟨fun h' => h'.elim id fun ⟨b, _, _, _, hb⟩ => (h b N hb).elim, .inl⟩

theorem Caps.substT_comp (C : Caps) (σ σ' : Atom → Ty) :
    (C.substT σ).substT σ' = C.substT fun a => (σ a).substT σ' := by
  funext a; apply propext
  simp only [Caps.substT, Ty.reach_substT]
  constructor
  · rintro ⟨b, ⟨c, hc, hb⟩, h⟩; exact ⟨c, hc, b, hb, h⟩
  · rintro ⟨c, hc, b, hb, h⟩; exact ⟨b, ⟨c, hc, hb⟩, h⟩

theorem Ty.asRow_substT_iff (σ : Atom → Ty) (t : Ty) (x : Elem) :
    (t.substT σ).asRow x ↔ t.asRow.substT σ x := by rw [Ty.asRow_substT]

/-- Substitutions of a row compose. A variable taken without `G`, whose instance holds a
variable without `H'`, becomes that one without both; `∨` is associative. -/
theorem Row.substT_comp (r : Row) (σ σ' : Atom → Ty) :
    (r.substT σ).substT σ' = r.substT fun a => (σ a).substT σ' := by
  have hr := fun t a => Ty.reach_substT σ' t a
  have hs := fun t x => Ty.asRow_substT_iff σ' t x
  funext x; apply propext
  cases x with
  | nm N =>
    constructor
    · rintro ((h | ⟨b, G, hb, hG, h⟩) | ⟨c, K, ⟨b, G, hb, H', hc, rfl⟩, hK, h⟩)
      · exact .inl h
      · exact .inr ⟨b, G, hb, hG, (hs _ _).mpr (.inl h)⟩
      · exact .inr ⟨b, G, hb, fun h' => hK (.inl h'),
          (hs _ _).mpr (.inr ⟨c, H', hc, fun h' => hK (.inr h'), h⟩)⟩
    · rintro (h | ⟨b, G, hb, hG, h⟩)
      · exact .inl (.inl h)
      · rcases (hs _ _).mp h with h | ⟨c, H', hc, hH, h⟩
        · exact .inl (.inr ⟨b, G, hb, hG, h⟩)
        · exact .inr ⟨c, _, ⟨b, G, hb, H', hc, rfl⟩, fun h' => h'.elim hG hH, h⟩
  | atm a =>
    constructor
    · rintro (⟨c, (⟨b, hb, hc⟩ | ⟨b, G, hb, hc⟩), h⟩ | ⟨c, K, ⟨b, G, hb, H', hc, -⟩, h⟩)
      · exact .inl ⟨b, hb, (hr _ a).mpr ⟨c, hc, h⟩⟩
      · exact .inr ⟨b, G, hb, (hs _ _).mpr (.inl ⟨c, hc, h⟩)⟩
      · exact .inr ⟨b, G, hb, (hs _ _).mpr (.inr ⟨c, H', hc, h⟩)⟩
    · rintro (⟨b, hb, h⟩ | ⟨b, G, hb, h⟩)
      · obtain ⟨c, hc, h⟩ := (hr _ a).mp h
        exact .inl ⟨c, .inl ⟨b, hb, hc⟩, h⟩
      · rcases (hs _ _).mp h with ⟨c, hc, h⟩ | ⟨c, H', hc, h⟩
        · exact .inl ⟨c, .inr ⟨b, G, hb, hc⟩, h⟩
        · exact .inr ⟨c, _, ⟨b, G, hb, H', hc, rfl⟩, h⟩
  | ob N a =>
    constructor
    · rintro (⟨c, (⟨b, hb, hc⟩ | ⟨w, G, b, hb, hG, hw, hc⟩), h⟩ |
        ⟨v, K, c, ⟨w, G, b, hb, ⟨H', hv, rfl⟩, hc⟩, hK, hn, h⟩)
      · exact .inl ⟨b, hb, (hr _ a).mpr ⟨c, hc, h⟩⟩
      · exact .inr ⟨w, G, b, hb, hG, (hs _ _).mpr (.inl hw), (hr _ a).mpr ⟨c, hc, h⟩⟩
      · exact .inr ⟨w, G, b, hb, fun h' => hK (.inl h'),
          (hs _ _).mpr (.inr ⟨v, H', hv, fun h' => hK (.inr h'), hn⟩), (hr _ a).mpr ⟨c, hc, h⟩⟩
    · rintro (⟨b, hb, h⟩ | ⟨w, G, b, hb, hG, hw, h⟩)
      · obtain ⟨c, hc, h⟩ := (hr _ a).mp h
        exact .inl ⟨c, .inl ⟨b, hb, hc⟩, h⟩
      · obtain ⟨c, hc, h⟩ := (hr _ a).mp h
        rcases (hs _ _).mp hw with hw | ⟨v, H', hv, hH, hn⟩
        · exact .inl ⟨c, .inr ⟨w, G, b, hb, hG, hw, hc⟩, h⟩
        · exact .inr ⟨v, _, c, ⟨w, G, b, hb, ⟨H', hv, rfl⟩, hc⟩, fun h' => h'.elim hG hH, hn, h⟩
  | obv v₀ H₀ a =>
    constructor
    · rintro ⟨v, K, c, ⟨w, G, b, hb, ⟨H', hv, rfl⟩, hc⟩, ⟨H'', hv', rfl⟩, h⟩
      exact ⟨w, G, b, hb, ⟨fun n => H' n ∨ H'' n, (hs _ _).mpr ⟨v, H', hv, H'', hv', rfl⟩,
        by funext n; exact propext or_assoc⟩, (hr _ a).mpr ⟨c, hc, h⟩⟩
    · rintro ⟨w, G, b, hb, ⟨J, hJ, rfl⟩, h⟩
      obtain ⟨c, hc, h⟩ := (hr _ a).mp h
      obtain ⟨v, H', hv, H'', hv', rfl⟩ := (hs _ _).mp hJ
      exact ⟨v, _, c, ⟨w, G, b, hb, ⟨H', hv, rfl⟩, hc⟩,
        ⟨H'', hv', by funext n; exact propext or_assoc.symm⟩, h⟩
  | rv c₀ H₀ =>
    constructor
    · rintro ⟨c, K, ⟨b, G, hb, H', hc, rfl⟩, H'', h, rfl⟩
      exact ⟨b, G, hb, fun n => H' n ∨ H'' n, (hs _ _).mpr ⟨c, H', hc, H'', h, rfl⟩,
        by funext n; exact propext or_assoc⟩
    · rintro ⟨b, G, hb, J, hJ, rfl⟩
      obtain ⟨c, H', hc, H'', h, rfl⟩ := (hs _ _).mp hJ
      exact ⟨c, _, ⟨b, G, hb, H', hc, rfl⟩, H'', h, by funext n; exact propext or_assoc.symm⟩

theorem Stage.substT_comp (φ : Stage) (σ σ' : Atom → Ty) :
    (φ.substT σ).substT σ' = φ.substT fun a => (σ a).substT σ' := by
  cases φ <;> simp [Stage.substT, Row.substT_comp]

theorem Caps.substT_ident (C : Caps) : C.substT Ty.ident = C := by
  funext a; apply propext; simp [Caps.substT]

theorem Ty.asRow_ident (b : Atom) : (Ty.ident b).asRow = fun x => x = .rv b fun _ => False := by
  cases b <;> rfl

theorem Row.substT_ident (r : Row) : r.substT Ty.ident = r := by
  funext x; apply propext; cases x <;> simp [Row.substT, Ty.asRow_ident, Row.less_var]

theorem Stage.substT_ident (φ : Stage) : φ.substT Ty.ident = φ := by
  cases φ <;> simp [Stage.substT, Row.substT_ident]

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
  | _ =>
    simp_all [Ty.ren, Ty.substT, Caps.substT_comp, Row.substT_comp, Stage.substT_comp, Atom.ty_substT]

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
  | _ =>
    simp_all [Ty.substT, Caps.substT_comp, Row.substT_comp, Stage.substT_comp, Ty.ren_eq]

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
  | _ => simp_all [Ty.substT, Caps.substT_comp, Row.substT_comp, Stage.substT_comp]

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
  | _ => simp_all [Ty.substT, Ty.ident, Caps.substT_ident, Row.substT_ident, Stage.substT_ident]

end Flick
