import Formal.Syntax

/-!
# A row variable without names (core.md §1.2)

`Row.less G r` is the row `r` less the names of `G`, which a substitution puts where a
row variable stands without them (`Elem.rv a G`). Here are its elements, what a
substituted row reaches, and the substitution of a variable alone.
-/

namespace Flick

@[simp] theorem Row.less_nm (G : Name → Prop) (r : Row) (n : Name) :
    r.less G (.nm n) ↔ ¬ G n ∧ r (.nm n) := Iff.rfl
@[simp] theorem Row.less_atm (G : Name → Prop) (r : Row) (a : Atom) :
    r.less G (.atm a) ↔ r (.atm a) := Iff.rfl
@[simp] theorem Row.less_ob (G : Name → Prop) (r : Row) (N : Name) (a : Atom) :
    ¬ r.less G (.ob N a) := id
@[simp] theorem Row.less_obv (G H : Name → Prop) (r : Row) (v a : Atom) :
    ¬ r.less G (.obv v H a) := id
theorem Row.less_rv (G : Name → Prop) (r : Row) (c : Atom) (H : Name → Prop) :
    r.less G (.rv c H) ↔ ∃ H', r (.rv c H') ∧ H = fun n => G n ∨ H' n := Iff.rfl

/-- A row variable less `G` is that variable without `G`. -/
theorem Row.less_var (G : Name → Prop) (b c : Atom) (H : Name → Prop) :
    Row.less G (fun x => x = .rv b fun _ => False) (.rv c H) ↔ c = b ∧ H = G := by
  constructor
  · rintro ⟨H', h, rfl⟩; cases h; exact ⟨rfl, by simp⟩
  · rintro ⟨rfl, rfl⟩; exact ⟨_, rfl, by simp⟩

/-- A row less no name is itself. -/
@[simp] theorem Row.less_none (r : Row) (c : Atom) (H : Name → Prop) :
    r.less (fun _ => False) (.rv c H) ↔ r (.rv c H) := by
  simp [Row.less_rv]

/-- `⌈τ⌉` is what `τ` stands for in a row, as atoms and as row variables. -/
theorem Ty.reach_asRow (t : Ty) (a : Atom) :
    t.reach a ↔ t.asRow (.atm a) ∨ ∃ G, t.asRow (.rv a G) := by
  induction t <;>
    simp_all [Ty.asRow, Ty.reach, Row.plain, Row.ofCaps, Row.none, Row.single, Row.union,
      Caps.none, Row.less] <;> grind

/-- What a substituted row reaches: the atoms of the instances of its atoms and of its
variables, whatever names these are taken without. -/
theorem Row.substT_reach (σ : Atom → Ty) (r : Row) (a : Atom) :
    (r.substT σ (.atm a) ∨ ∃ G, r.substT σ (.rv a G)) ↔
      ∃ b, (r (.atm b) ∨ ∃ G, r (.rv b G)) ∧ (σ b).reach a := by
  simp only [Row.substT, Ty.reach_asRow]
  constructor
  · rintro ((⟨b, hb, h⟩ | ⟨b, G, hb, h⟩) | ⟨H, b, G, hb, H', h, -⟩)
    · exact ⟨b, .inl hb, h⟩
    · exact ⟨b, .inr ⟨G, hb⟩, .inl h⟩
    · exact ⟨b, .inr ⟨G, hb⟩, .inr ⟨H', h⟩⟩
  · rintro ⟨b, hb | ⟨G, hb⟩, h⟩
    · exact .inl (.inl ⟨b, hb, h⟩)
    · rcases h with h | ⟨H', h⟩
      · exact .inl (.inr ⟨b, G, hb, h⟩)
      · exact .inr ⟨_, b, G, hb, H', h, rfl⟩

@[simp] theorem Ty.asRow_ob (t : Ty) (N : Name) (a : Atom) : ¬ t.asRow (.ob N a) := by
  induction t <;> simp_all [Ty.asRow, Row.plain, Row.ofCaps, Row.none, Row.single, Row.union]

@[simp] theorem Ty.asRow_obv (t : Ty) (v a : Atom) (G : Name → Prop) : ¬ t.asRow (.obv v G a) := by
  induction t <;> simp_all [Ty.asRow, Row.plain, Row.ofCaps, Row.none, Row.single, Row.union]

/-- A row variable alone becomes the row its instance stands for. -/
theorem Row.var_substT (σ : Atom → Ty) (b : Atom) :
    Row.substT σ (fun x => x = .rv b fun _ => False) = (σ b).asRow := by
  funext x; apply propext
  cases x with
  | nm N =>
    constructor
    · rintro (h | ⟨_, _, h, -, h'⟩)
      · cases h
      · cases h; exact h'
    · exact fun h => .inr ⟨b, _, rfl, id, h⟩
  | atm a =>
    constructor
    · rintro (⟨_, h, -⟩ | ⟨_, _, h, h'⟩)
      · cases h
      · cases h; exact h'
    · exact fun h => .inr ⟨b, _, rfl, h⟩
  | ob N a =>
    refine ⟨fun h => ?_, fun h => (Ty.asRow_ob _ _ _ h).elim⟩
    rcases h with ⟨_, h, -⟩ | ⟨_, _, _, h, -⟩ <;> cases h
  | obv v G a => exact ⟨fun ⟨_, _, _, h, _⟩ => (nomatch h), fun h => (Ty.asRow_obv _ _ _ _ h).elim⟩
  | rv c H =>
    constructor
    · rintro ⟨_, _, h, h'⟩; cases h; exact (Row.less_none _ _ _).mp h'
    · exact fun h => ⟨b, _, rfl, (Row.less_none _ _ _).mpr h⟩

end Flick
