import Formal.Elab.CoreCgOps

/-!
# λFlick⁺ as a `Target`: related evaluation contexts (P4a4b)

`FrR B F F'`: two frames of one kind with related parts. Related contexts plug related terms
into related terms (`FrR.plugs`), keep the order of evaluation (`FrR.okE`), the services of
their `with` frames (`FrR.lookupE`) and the environment a child inherits (`FrR.withs`).
A related term around a hole is a base pair at some frame, or the same frames around a
related hole (`Cg.decomp`).
-/

namespace Flick.Elab

/-- Two frames of one kind with related parts. -/
inductive FrR (B : Flick.Tm → Flick.Tm → Prop) : Frame → Frame → Prop
  | appL {e e'} : Cg B false e e' → FrR B (.appL e) (.appL e')
  | appR {v v'} : Cg B false v v' → FrR B (.appR v) (.appR v')
  | let_ {b b'} : Cg B false b b' → FrR B (.let_ b) (.let_ b')
  | match_ {al al'} : Cg B false al al' → FrR B (.match_ al) (.match_ al')
  | rconsL {l r r'} : Cg B false r r' → FrR B (.rconsL l r) (.rconsL l r')
  | rconsR {l v v'} : Cg B false v v' → FrR B (.rconsR l v) (.rconsR l v')
  | proj {l} : FrR B (.proj l) (.proj l)
  | updL {l e e'} : Cg B false e e' → FrR B (.updL l e) (.updL l e')
  | updR {v v' l} : Cg B false v v' → FrR B (.updR v l) (.updR v' l)
  | await : FrR B .await .await
  | try_ {al al'} : Cg B false al al' → FrR B (.try_ al) (.try_ al')
  | provide {e e'} : Cg B false e e' → FrR B (.provide e) (.provide e')
  | bind {S} : FrR B (.bind S) (.bind S)
  | joinL {e e'} : Cg B false e e' → FrR B (.joinL e) (.joinL e')
  | joinR {v v'} : Cg B false v v' → FrR B (.joinR v) (.joinR v')
  | with_ {q θ θ'} : Cg B false θ θ' → FrR B (.with_ q θ) (.with_ q θ')
  | raise : FrR B .raise .raise
  | inside {p} : FrR B (.inside p) (.inside p)

variable {B : Flick.Tm → Flick.Tm → Prop}

theorem FrR.plug {F F' : Frame} (h : FrR B F F') {a a' : Flick.Tm} (ha : Cg B false a a') :
    Cg B false (F.plug a) (F'.plug a') := by
  cases h <;> simp only [Frame.plug] <;> constructor <;> assumption

theorem FrR.ok (hB : BNoVal B) {F F' : Frame} (h : FrR B F F') (hF : F.ok) : F'.ok := by
  cases h <;> simp only [Frame.ok] at hF ⊢ <;> first | trivial | exact (Cg.value hB hF ‹_›).1

theorem FrR.isTry {F F' : Frame} (h : FrR B F F') : F.isTry ↔ F'.isTry := by
  cases h <;> simp [Frame.isTry]

theorem FrR.isInside {F F' : Frame} (h : FrR B F F') : F.isInside ↔ F'.isInside := by
  cases h <;> simp [Frame.isInside]

theorem FrR.wrap {F F' : Frame} (h : FrR B F F') {a a' : Flick.Tm} (ha : Cg B false a a') :
    Cg B false (F.wrap a) (F'.wrap a') := by
  cases h <;> simp only [Frame.wrap] <;> first | exact ha | exact .with_ ‹_› ha

/-- Related services: one region and related values. -/
def SvR (B : Flick.Tm → Flick.Tm → Prop) (a b : Nat × Flick.Tm) : Prop :=
  a.1 = b.1 ∧ Cg B false a.2 b.2

theorem FrR.lookup (hB : BNoVal B) {F F' : Frame} (h : FrR B F F') (hF : F.ok) (S : Name) :
    ORel (SvR B) (F.lookup S) (F'.lookup S) := by
  cases h with
  | with_ hθ =>
    simp only [Frame.lookup]
    have := hθ.envLookup hB hF S
    revert this
    cases (_ : Flick.Tm).envLookup S <;> cases (_ : Flick.Tm).envLookup S <;>
      simp [ORel, SvR]
  | _ => simp [Frame.lookup, ORel]

theorem FrR.plugs {E E' : List Frame} (h : All2 (FrR B) E E') :
    ∀ {a a' : Flick.Tm}, Cg B false a a' → Cg B false (Flick.plug E a) (Flick.plug E' a') := by
  induction h with
  | nil => exact id
  | cons hF _ ih => exact fun ha => ih (hF.plug ha)

theorem FrR.okE (hB : BNoVal B) {E E' : List Frame} (h : All2 (FrR B) E E') (hE : OkE E) :
    OkE E' := by
  induction h with
  | nil => exact OkE.nil
  | cons hF _ ih =>
    obtain ⟨h₁, h₂⟩ := hE.cons
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · exact hF.ok hB h₁
    · exact ih h₂ G hG

theorem FrR.lookupE (hB : BNoVal B) {E E' : List Frame} (h : All2 (FrR B) E E') (hE : OkE E)
    (S : Name) : ORel (SvR B) (Flick.lookupE E S) (Flick.lookupE E' S) := by
  induction h with
  | nil => trivial
  | cons hF _ ih =>
    obtain ⟨h₁, h₂⟩ := hE.cons
    have hl := hF.lookup hB h₁ S
    simp only [Flick.lookupE]
    revert hl
    cases Frame.lookup _ S <;> cases Frame.lookup _ S <;> simp [ORel] <;> first
      | exact ih h₂
      | exact id

theorem FrR.withs {E E' : List Frame} (h : All2 (FrR B) E E') :
    ∀ {a a' : Flick.Tm}, Cg B false a a' → Cg B false (Flick.withs E a) (Flick.withs E' a') := by
  induction h with
  | nil => exact id
  | cons hF _ ih => exact fun ha => ih (hF.wrap ha)

/-- A related term in a frame is a base pair, or the same frame around a related hole. -/
theorem Cg.frame {F : Frame} {a t : Flick.Tm} (h : Cg B false (F.plug a) t) :
    B (F.plug a) t ∨ ∃ F' a', t = F'.plug a' ∧ FrR B F F' ∧ Cg B false a a' := by
  cases F <;> simp only [Frame.plug] at h <;> cases h <;>
    first
    | exact .inl ‹_›
    | exact .inr ⟨_, _, rfl, by constructor <;> assumption, ‹_›⟩

/-- **Decomposition**: a related term around a hole is a base pair at some frame, or the same
frames around a related hole. -/
theorem Cg.decomp : ∀ {E : List Frame} {r t : Flick.Tm}, Cg B false (Flick.plug E r) t →
    (∃ E₀ E₁ E₂ x₂, E = E₀ ++ E₁ ∧ B (Flick.plug E₀ r) x₂ ∧ t = Flick.plug E₂ x₂ ∧
      All2 (FrR B) E₁ E₂) ∨
    ∃ E₂ r₂, t = Flick.plug E₂ r₂ ∧ All2 (FrR B) E E₂ ∧ Cg B false r r₂
  | [], _, _, h => .inr ⟨[], _, rfl, .nil, h⟩
  | F :: E, r, t, h => by
    rcases Cg.decomp (E := E) h with ⟨E₀, E₁, E₂, x₂, rfl, hb, rfl, h₁⟩ | ⟨E₂, r₂, rfl, h₁, h₂⟩
    · exact .inl ⟨F :: E₀, E₁, E₂, x₂, rfl, hb, rfl, h₁⟩
    · rcases h₂.frame with hb | ⟨F', a', rfl, hF, ha⟩
      · exact .inl ⟨[F], E, E₂, _, rfl, hb, rfl, h₁⟩
      · exact .inr ⟨F' :: E₂, a', rfl, .cons hF h₁, ha⟩

end Flick.Elab
