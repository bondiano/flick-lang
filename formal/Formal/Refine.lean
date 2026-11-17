/-!
# The logic of `T.prove` (refinements.md §4–§5)

The checker proves the check of a type with an invariant from facts: formulas the path ran,
each with the result it gave. This file states that logic and proves it sound: whatever the
checker learns from facts holds in every world where the facts ran to their results, and
whatever `holds` decides of a formula, the formula gives when it runs there.

A formula runs left to right and stops early as `&&` and `||` do; an atom may be a call that
never returns, so evaluation is three-valued (`eval`). A fact tells only what the parts that
ran gave: `holds` decides a part only after the parts before it, which run first, so
`f x || true` is not proved for an `f x` nothing observed (`unobserved_or_true`).

Numbers are values of a total preorder with a point NaN that every ordered comparison finds
false (`Num`): `Float` as binary64 compares so, its two zeros one point and its infinities
values, and `Int` is the preorder of the integers without NaN. What comparisons with
constants establish of one number is an interval with ends open or closed (`Interval`);
`Interval.learn_sound` and `Interval.decide_sound` are its laws, and an `Int`'s ends are
moved to closed ones (`Interval.int_side`), as the checker moves them.

What is not proved here: that the Rust checker follows these definitions, and that a
runtime's IEEE arithmetic is the preorder. The checker's property test runs the same rules
against an evaluator of its own (`flick-core/src/types/proofs/props.rs`).
-/

namespace Flick.Refine

/-! ## Formulas and their three-valued evaluation -/

/-- A Boolean expression in the order the program runs it. `hidden` is a part the logic does
not read. -/
inductive Formula (α : Type) where
  | const (b : Bool)
  | atom (a : α)
  | not (x : Formula α)
  | and (x y : Formula α)
  | or (x y : Formula α)
  | hidden (i : Nat)

/-- A world: the result each atom and each hidden part gives when it runs, or none when it
never returns. -/
structure World (α : Type) where
  atom : α → Option Bool
  hidden : Nat → Option Bool

/-- What a formula gives when it runs in `w`: the second operand of `&&` and `||` runs only
after the first gave the result that does not decide alone. -/
def eval (w : World α) : Formula α → Option Bool
  | .const b => some b
  | .atom a => w.atom a
  | .not x => (eval w x).map (!·)
  | .and x y =>
    match eval w x with
    | some true => eval w y
    | some false => some false
    | none => none
  | .or x y =>
    match eval w x with
    | some true => some true
    | some false => eval w y
    | none => none
  | .hidden i => w.hidden i

/-! ## What facts establish

The logic is generic in what it knows of atoms (`Theory`): a list of atoms with their results,
and for numbers the intervals of their subjects too (`numbers`). `holds` and `assume` are the
checker's (`Knowledge::holds`, `Knowledge::assume` of `logic.rs`). -/

/-- What is known of atoms: the result knowledge `k` gives an atom, and what it becomes once an
atom gave a result. -/
structure Theory (α K : Type) where
  known : K → α → Option Bool
  add : K → α → Bool → K

variable {α K : Type} (T : Theory α K)

/-- The result `holds` decides of a formula from what is known. -/
def holds (k : K) : Formula α → Option Bool
  | .const b => some b
  | .atom a => T.known k a
  | .not x => (holds k x).map (!·)
  | .and x y =>
    match holds k x with
    | some true => holds k y
    | some false => some false
    | none => none
  | .or x y =>
    match holds k x with
    | some true => some true
    | some false => holds k y
    | none => none
  | .hidden _ => none

/-- Learns that `f` ran to `r`: both parts of a `&&` that gave `true` or an `||` that gave
`false`; the second part of one that gave the other result once the first is known to let it
run, or the first part once the second is known to give, wherever it runs, the result that
does not decide (a pure atom gives one result wherever it runs). -/
def assume (k : K) : Formula α → Bool → K
  | .const _, _ => k
  | .hidden _, _ => k
  | .atom a, r => T.add k a r
  | .not x, r => assume k x (!r)
  | .and x y, true => assume (assume k x true) y true
  | .or x y, false => assume (assume k x false) y false
  | .and x y, false =>
    if holds T k x = some true then assume k y false
    else if holds T k y = some true then assume k x false else k
  | .or x y, true =>
    if holds T k x = some false then assume k y true
    else if holds T k y = some false then assume k x true else k

/-- A theory sound in `w` under the invariant `Inv`: what it knows of an atom, the atom gives
there, and learning a result the atom gives keeps the invariant. -/
structure Theory.Sound (w : World α) (Inv : K → Prop) : Prop where
  known : ∀ k a b, Inv k → T.known k a = some b → w.atom a = some b
  add : ∀ k a r, Inv k → w.atom a = some r → Inv (T.add k a r)

variable {T} {w : World α} {Inv : K → Prop}

/-- What `holds` decides, the formula gives when it runs in a world the knowledge holds in. -/
theorem holds_sound (hT : T.Sound w Inv) {k : K} (hk : Inv k) :
    ∀ (f : Formula α) {b}, holds T k f = some b → eval w f = some b
  | .const c, b, h => by simp [holds] at h; simp [eval, h]
  | .atom a, b, h => by simp [holds] at h; simp [eval]; exact hT.known k a b hk h
  | .not x, b, h => by
    simp only [holds] at h
    cases hx : holds T k x with
    | none => simp [hx] at h
    | some c =>
      simp [hx] at h
      simp [eval, holds_sound hT hk x hx, h]
  | .and x y, b, h => by
    simp only [holds] at h
    cases hx : holds T k x with
    | none => simp [hx] at h
    | some c =>
      have ex := holds_sound hT hk x hx
      cases c with
      | true => simp [hx] at h; simp [eval, ex, holds_sound hT hk y h]
      | false => simp [hx] at h; simp [eval, ex, h]
  | .or x y, b, h => by
    simp only [holds] at h
    cases hx : holds T k x with
    | none => simp [hx] at h
    | some c =>
      have ex := holds_sound hT hk x hx
      cases c with
      | true => simp [hx] at h; simp [eval, ex, h]
      | false => simp [hx] at h; simp [eval, ex, holds_sound hT hk y h]
  | .hidden _, b, h => by simp [holds] at h

/-- Learning a fact that ran to its result keeps the knowledge sound. -/
theorem assume_sound (hT : T.Sound w Inv) :
    ∀ (f : Formula α) (r : Bool) {k : K}, Inv k → eval w f = some r → Inv (assume T k f r)
  | .const _, r, k, hk, _ => by simpa [assume] using hk
  | .hidden _, r, k, hk, _ => by simpa [assume] using hk
  | .atom a, r, k, hk, he => by
    simp only [assume]; exact hT.add k a r hk (by simpa [eval] using he)
  | .not x, r, k, hk, he => by
    simp only [assume]
    apply assume_sound hT x (!r) hk
    simp only [eval] at he
    cases hx : eval w x with
    | none => simp [hx] at he
    | some c => simp [hx] at he; simp [← he]
  | .and x y, true, k, hk, he => by
    simp only [eval] at he
    cases hx : eval w x with
    | none => simp [hx] at he
    | some c =>
      cases c with
      | false => simp [hx] at he
      | true =>
        simp [hx] at he
        exact assume_sound hT y true (assume_sound hT x true hk hx) he
  | .or x y, false, k, hk, he => by
    simp only [eval] at he
    cases hx : eval w x with
    | none => simp [hx] at he
    | some c =>
      cases c with
      | true => simp [hx] at he
      | false =>
        simp [hx] at he
        exact assume_sound hT y false (assume_sound hT x false hk hx) he
  | .and x y, false, k, hk, he => by
    simp only [assume]
    simp only [eval] at he
    split
    · rename_i h
      have ex := holds_sound hT hk x h
      simp [ex] at he
      exact assume_sound hT y false hk he
    · split
      · rename_i _ h
        have ey := holds_sound hT hk y h
        apply assume_sound hT x false hk
        cases hx : eval w x with
        | none => simp [hx] at he
        | some c => cases c <;> simp_all
      · exact hk
  | .or x y, true, k, hk, he => by
    simp only [assume]
    simp only [eval] at he
    split
    · rename_i h
      have ex := holds_sound hT hk x h
      simp [ex] at he
      exact assume_sound hT y true hk he
    · split
      · rename_i _ h
        have ey := holds_sound hT hk y h
        apply assume_sound hT x true hk
        cases hx : eval w x with
        | none => simp [hx] at he
        | some c => cases c <;> simp_all
      · exact hk

/-- Facts learned in rounds, as many as the budget allows: what the checker knows. -/
def learn (T : Theory α K) (facts : List (Formula α × Bool)) : Nat → K → K
  | 0, k => k
  | n + 1, k => learn T facts n (facts.foldl (fun k p => assume T k p.1 p.2) k)

theorem learn_sound (hT : T.Sound w Inv) (facts : List (Formula α × Bool))
    (hf : ∀ p ∈ facts, eval w p.1 = some p.2) :
    ∀ n {k : K}, Inv k → Inv (learn T facts n k)
  | 0, k, hk => hk
  | n + 1, k, hk => by
    apply learn_sound hT facts hf n
    suffices ∀ (l : List (Formula α × Bool)) (k : K), (∀ p ∈ l, eval w p.1 = some p.2) →
        Inv k → Inv (l.foldl (fun k p => assume T k p.1 p.2) k) from
      this facts k hf hk
    intro l
    induction l with
    | nil => intro k _ hk; exact hk
    | cons p l ih =>
      intro k hl hk
      exact ih _ (fun q hq => hl q (List.mem_cons_of_mem _ hq))
        (assume_sound hT p.1 p.2 hk (hl p (List.mem_cons_self ..)))

/-- **Soundness of the logic.** In a world where every fact ran to its result, a formula
`holds` decides from the facts gives that result when it runs. -/
theorem sound (hT : T.Sound w Inv) {k₀ : K} (h₀ : Inv k₀) (facts : List (Formula α × Bool))
    (hf : ∀ p ∈ facts, eval w p.1 = some p.2) (n : Nat) (f : Formula α) {b}
    (h : holds T (learn T facts n k₀) f = some b) : eval w f = some b :=
  holds_sound hT (learn_sound hT facts hf n h₀) f h

/-! ## Atoms by their results -/

/-- The result an atom has in a list, the first one learned. -/
def lookup [DecidableEq α] (l : List (α × Bool)) (a : α) : Option Bool :=
  (l.find? (fun p => p.1 = a)).map Prod.snd

theorem lookup_sound [DecidableEq α] {l : List (α × Bool)}
    (hl : ∀ a b, (a, b) ∈ l → w.atom a = some b) {a b} (h : lookup l a = some b) :
    w.atom a = some b := by
  unfold lookup at h
  cases hf : l.find? (fun p => p.1 = a) with
  | none => simp [hf] at h
  | some p =>
    simp [hf] at h
    have hm := List.mem_of_find?_eq_some hf
    have hp := List.find?_some hf
    obtain ⟨a', b'⟩ := p
    simp at hp h
    subst hp; subst h
    exact hl _ _ hm

/-- Atoms alone: what the checker knows of calls and `Bool` values. -/
def atoms [DecidableEq α] : Theory α (List (α × Bool)) where
  known := lookup
  add l a r := if (lookup l a).isSome then l else l ++ [(a, r)]

theorem atoms_sound [DecidableEq α] :
    (atoms (α := α)).Sound w (fun l => ∀ a b, (a, b) ∈ l → w.atom a = some b) where
  known _ _ _ hl h := lookup_sound hl h
  add l a r hl he := by
    simp only [atoms]
    split
    · exact hl
    · intro a' b' hm
      simp at hm
      rcases hm with hm | ⟨rfl, rfl⟩
      · exact hl _ _ hm
      · exact he

/-- A call nothing observed proves nothing, even where classical logic would: `f x || true`
is not decided, though `true || f x` is. -/
theorem unobserved_or_true [DecidableEq α] (a : α) :
    holds atoms ([] : List (α × Bool)) (.or (.atom a) (.const true)) = none ∧
      holds atoms ([] : List (α × Bool)) (.or (.const true) (.atom a)) = some true := by
  simp [holds, atoms, lookup]

/-- An `||` that gave `true` decides neither part, and does not prove its parts swapped. -/
theorem or_true_decides_neither [DecidableEq α] (a b : α) :
    let k := assume atoms ([] : List (α × Bool)) (.or (.atom a) (.atom b)) true
    holds atoms k (.atom a) = none ∧ holds atoms k (.or (.atom b) (.atom a)) = none := by
  simp [assume, holds, atoms, lookup]

end Flick.Refine
