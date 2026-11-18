import Formal.RowLess

/-!
# What a row term stands for under a substitution

`Ty.asRow` commutes with substitution (`Ty.asRow_substT`), and substitutions of predicate rows
compose (`Row.substT_comp`). The syntactic laws of substitution are in `TyReach.lean`.
-/

namespace Flick

theorem Atom.ty_asRow (b : Atom) (x : Elem) : b.ty.asRow x ↔ x = .rv b fun _ => False := by
  cases b <;> simp [Atom.ty, Ty.asRow]

theorem Row.ofCaps_substT (C : Caps) (σ : Atom → Ty) :
    (Row.ofCaps C).substT σ = Row.ofCaps (C.substT σ) := by
  funext x; cases x <;> simp [Row.substT, Row.ofCaps, Caps.substT]

theorem Ty.asRow_tvar (n : Nat) : (Ty.tvar n).asRow = fun x => x = .rv (.v n) fun _ => False :=
  rfl
theorem Ty.asRow_rg (p : Nat) : (Ty.rg p).asRow = fun x => x = .rv (.r p) fun _ => False := rfl

@[simp] theorem Row.none_substT (σ : Atom → Ty) : Row.none.substT σ = Row.none := by
  funext x; cases x <;> simp [Row.substT, Row.none]

@[simp] theorem Row.inst_substT (σ : Atom → Ty) (N : Name) (c : TyClass) :
    (Row.inst N c).substT σ = Row.inst N (c.substT σ) := by
  funext x; apply propext
  cases x with
  | nm M d => simp only [Row.substT, Row.inst]; grind
  | _ => simp [Row.substT, Row.inst]

@[simp] theorem Row.single_substT (σ : Atom → Ty) (N : Name) :
    (Row.single N).substT σ = Row.single N := by
  simp only [Row.single, Row.inst_substT, TyClass.substT_classOf, Ty.substT]

@[simp] theorem Row.union_substT (σ : Atom → Ty) (r r' : Row) :
    (Row.union r r').substT σ = Row.union (r.substT σ) (r'.substT σ) := by
  funext x; apply propext
  cases x <;> simp only [Row.substT, Row.union] <;> grind

theorem Row.less_substT (G : Name → Prop) (r : Row) (σ : Atom → Ty) :
    (r.less G).substT σ = (r.substT σ).less G := by
  funext x; apply propext
  cases x with
  | nm N c | atm a => simp only [Row.less, Row.substT]; grind
  | ob N c a | obv v H a => simp [Row.less, Row.substT]
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
  | cnil => simp [Ty.substT, Ty.asRow]
  | rnm N args ih =>
    simp only [Ty.substT, Ty.asRow, Row.union_substT, Row.inst_substT, TyClass.substT_classOf,
      Row.ofCaps_substT, Caps.substT_reach]
  | run left right ihleft ihright =>
    simp only [Ty.substT, Ty.asRow, ihleft, ihright, Row.union_substT]
  | rless gone r ih =>
    simp only [Ty.substT, Ty.asRow, ih, Row.less_substT]
  | _ => exact hc _ rfl rfl

/-- A substitution that brings no name into a row: the instance of each row variable has
no service and no error. -/
def NoNames (σ : Atom → Ty) : Prop := ∀ b N c, ¬ (σ b).asRow (.nm N c)

theorem Row.substT_nm {σ : Atom → Ty} (h : NoNames σ) (r : Row) (N : Name) (c : TyClass) :
    r.substT σ (.nm N c) ↔ ∃ c₀, r (.nm N c₀) ∧ c₀.substT σ = c :=
  ⟨fun h' => h'.elim id fun ⟨b, _, _, _, hb⟩ => (h b N c hb).elim, .inl⟩

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
  | nm N d =>
    constructor
    · rintro (⟨d₁, (⟨d₀, h, rfl⟩ | ⟨b, G, hb, hG, h⟩), rfl⟩ | ⟨c, K, ⟨b, G, hb, H', hc, rfl⟩, hK, h⟩)
      · exact .inl ⟨d₀, h, (TyClass.substT_comp _ _ _).symm⟩
      · exact .inr ⟨b, G, hb, hG, (hs _ _).mpr (.inl ⟨_, h, rfl⟩)⟩
      · exact .inr ⟨b, G, hb, fun h' => hK (.inl h'),
          (hs _ _).mpr (.inr ⟨c, H', hc, fun h' => hK (.inr h'), h⟩)⟩
    · rintro (⟨d₀, h, rfl⟩ | ⟨b, G, hb, hG, h⟩)
      · exact .inl ⟨_, .inl ⟨d₀, h, rfl⟩, TyClass.substT_comp _ _ _⟩
      · rcases (hs _ _).mp h with ⟨d₁, h₁, rfl⟩ | ⟨c, H', hc, hH, h⟩
        · exact .inl ⟨_, .inr ⟨b, G, hb, hG, h₁⟩, rfl⟩
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
  | ob N d a =>
    constructor
    · rintro (⟨d₁, c, (⟨d₀, b, hb, rfl, hc⟩ | ⟨w, G, b, hb, hG, hw, hc⟩), rfl, h⟩ |
        ⟨v, K, c, ⟨w, G, b, hb, ⟨H', hv, rfl⟩, hc⟩, hK, hn, h⟩)
      · exact .inl ⟨d₀, b, hb, (TyClass.substT_comp _ _ _).symm, (hr _ a).mpr ⟨c, hc, h⟩⟩
      · exact .inr ⟨w, G, b, hb, hG, (hs _ _).mpr (.inl ⟨_, hw, rfl⟩), (hr _ a).mpr ⟨c, hc, h⟩⟩
      · exact .inr ⟨w, G, b, hb, fun h' => hK (.inl h'),
          (hs _ _).mpr (.inr ⟨v, H', hv, fun h' => hK (.inr h'), hn⟩), (hr _ a).mpr ⟨c, hc, h⟩⟩
    · rintro (⟨d₀, b, hb, rfl, h⟩ | ⟨w, G, b, hb, hG, hw, h⟩)
      · obtain ⟨c, hc, h⟩ := (hr _ a).mp h
        exact .inl ⟨_, c, .inl ⟨d₀, b, hb, rfl, hc⟩, TyClass.substT_comp _ _ _, h⟩
      · obtain ⟨c, hc, h⟩ := (hr _ a).mp h
        rcases (hs _ _).mp hw with ⟨d₁, hw₁, rfl⟩ | ⟨v, H', hv, hH, hn⟩
        · exact .inl ⟨_, c, .inr ⟨w, G, b, hb, hG, hw₁, hc⟩, rfl, h⟩
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

theorem Ty.asRow_ident (b : Atom) : (Ty.ident b).asRow = fun x => x = .rv b fun _ => False := by
  cases b <;> rfl

theorem Row.substT_ident (r : Row) : r.substT Ty.ident = r := by
  funext x; apply propext; cases x <;> simp [Row.substT, Ty.asRow_ident, Row.less_var,
    TyClass.substT_ident]

theorem Stage.substT_ident (φ : Stage) : φ.substT Ty.ident = φ := by
  cases φ <;> simp [Stage.substT, Row.substT_ident]

end Flick
