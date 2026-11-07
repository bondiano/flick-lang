import Formal.Elab.CoreCgFrames
import Formal.Lemma3

/-!
# λFlick⁺ as a `Target`: related handlers

Related terms run one handler: a body related to a parked one has the same frames (`Cg.plug_hole`),
an operation has related arms (`Cg.armOf`), and related frames handle the same operations,
park the same parts and unwind to the same handler (`FrR.handles`, `FrR.isCtl`,
`FrR.catches`). For this a base pair holds no run-time form, so filling a hole leaves it, and
neither side is a list of alternatives (`BNoAlts`, `BPlain`).
-/

namespace Flick.Elab

def IsAlts : Flick.Tm → Prop
  | .anil | .acons .. | .aguard .. => True
  | _ => False

/-- The left side of a base pair is never a list of alternatives. -/
def BNoAlts (B : Flick.Tm → Flick.Tm → Prop) : Prop := ∀ a b, B a b → ¬ IsAlts a

/-- Base pairs hold no run-time form, and the right side is no list of alternatives. -/
def BPlain (B : Flick.Tm → Flick.Tm → Prop) : Prop :=
  ∀ a b, B a b → a.NoIns ∧ b.NoIns ∧ ¬ IsAlts b

variable {B : Flick.Tm → Flick.Tm → Prop}

/-- A body related to a body parked at its hole: the same frames around the hole. -/
theorem Cg.plug_hole (hP : BPlain B) {Ei : List Frame} {p : Nat} {k₂ : Flick.Tm}
    (h : Cg B false (Flick.plug Ei (.hole p)) k₂) :
    ∃ Ei₂, k₂ = Flick.plug Ei₂ (.hole p) ∧ All2 (FrR B) Ei Ei₂ := by
  rcases h.decomp with ⟨E₀, E₁, E₂, x₂, hE, hb, ht, hfr⟩ | ⟨Ei₂, r₂, rfl, hfr, hr⟩
  · exact ((hP _ _ hb).1 (plug_has E₀ (by simp [Flick.Tm.Has]))).elim
  · cases hr with
    | hole => exact ⟨Ei₂, rfl, hfr⟩
    | base hb => exact ((hP _ _ hb).1 (by simp [Flick.Tm.Has])).elim

theorem armOf_none {a : Flick.Tm} (h : ¬ IsAlts a) (o : Name) : a.armOf o = none := by
  cases a <;> simp_all [IsAlts, Flick.Tm.armOf]

/-- Related arms of an operation. -/
theorem Cg.armOf (hA : BNoAlts B) (hP : BPlain B) {o : Name} {x : Bool} {al al₂ : Flick.Tm}
    (h : Cg B x al al₂) : ORel (Cg B false) (al.armOf o) (al₂.armOf o) := by
  induction h with
  | @acons _ p _ _ _ _ hb _ _ ihr =>
    simp only [Flick.Tm.armOf]
    by_cases hc : p.headName = some o
    · simp only [hc, if_true]; exact hb
    · simp only [hc, if_false]; exact ihr
  | base hb => rw [armOf_none (hA _ _ hb), armOf_none (hP _ _ hb).2.2]; trivial
  | _ => simp [Flick.Tm.armOf, ORel]

theorem FrR.isCtl {F F' : Frame} (h : FrR B F F') : F.isCtl ↔ F'.isCtl := by
  cases h <;> simp [Frame.isCtl]

theorem FrR.catches {F F' : Frame} (h : FrR B F F') (p ℓ : Nat) : F.catches p ℓ ↔ F'.catches p ℓ := by
  cases h <;> simp [Frame.catches]

theorem FrR.resumes {F F' : Frame} (h : FrR B F F') (p : Nat) : F.resumes p ↔ F'.resumes p := by
  cases h <;> simp [Frame.resumes]

theorem FrR.resumesE {E E₂ : List Frame} (h : All2 (FrR B) E E₂) {p : Nat}
    (hn : ∀ G ∈ E, ¬ G.resumes p) : ∀ G ∈ E₂, ¬ G.resumes p := by
  induction h with
  | nil => intro G hG; cases hG
  | cons hF _ ih =>
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · exact fun h => hn _ (List.mem_cons_self ..) ((hF.resumes p).2 h)
    · exact ih (fun G' hG' => hn G' (List.mem_cons_of_mem _ hG')) G hG

theorem FrR.handles (hA : BNoAlts B) (hP : BPlain B) {F F' : Frame} (h : FrR B F F') (E o : Name)
    (c : OpClass) : F.handles E o c = F'.handles E o c := by
  cases h with
  | hwith hal _ =>
    have := Cg.armOf (o := o) hA hP hal
    simp only [Frame.handles]
    revert this
    cases (_ : Flick.Tm).armOf o <;> cases (_ : Flick.Tm).armOf o <;> simp [ORel]
  | _ => simp [Frame.handles]

theorem FrR.handlesE (hA : BNoAlts B) (hP : BPlain B) {E E₂ : List Frame} (h : All2 (FrR B) E E₂)
    {Ef o : Name} {c : OpClass} (hh : ∀ G ∈ E, G.handles Ef o c = none) : ∀ G ∈ E₂, G.handles Ef o c = none := by
  induction h with
  | nil => intro G hG; cases hG
  | cons hF _ ih =>
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · rw [← hF.handles hA hP]; exact hh _ (List.mem_cons_self ..)
    · exact ih (fun G' hG' => hh G' (List.mem_cons_of_mem _ hG')) G hG

theorem All2.split {α β : Type} {R : α → β → Prop} :
    ∀ {l₁ l₂ : List α} {l' : List β}, All2 R (l₁ ++ l₂) l' →
      ∃ m₁ m₂, l' = m₁ ++ m₂ ∧ All2 R l₁ m₁ ∧ All2 R l₂ m₂
  | [], _, _, h => ⟨[], _, rfl, .nil, h⟩
  | _ :: l₁, _, _, .cons r h => by
    obtain ⟨m₁, m₂, rfl, h₁, h₂⟩ := All2.split (l₁ := l₁) h
    exact ⟨_ :: m₁, m₂, rfl, .cons r h₁, h₂⟩

end Flick.Elab
