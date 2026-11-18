import Formal.Refine

/-!
# Numbers in the logic of `T.prove` (refinements.md §5)

What comparisons with constants establish of one number, `interval.rs` of the checker, and
the checker's knowledge of atoms and numbers together as a sound `Theory`, so `Refine.sound`
holds of it.
-/

namespace Flick.Refine

variable {α : Type} {w : World α}

/-! ## Numbers: an interval with a point NaN

`interval.rs`. A number is a value of a total preorder or NaN, which every ordered comparison
finds false; a comparison that gave `false` leaves NaN possible, so `not (x <= 0)` does not
prove `x > 0`. -/

/-- A total preorder with `lt a b` as `¬ le b a`: binary64 without NaN, whose two zeros are one
point and whose infinities are values, or the integers. -/
structure Num (V : Type) where
  le : V → V → Bool
  total : ∀ a b, le a b = true ∨ le b a = true
  trans : ∀ {a b c}, le a b = true → le b c = true → le a c = true

/-- An ordered comparison. -/
inductive Op | lt | le | gt | ge

/-- The values a number may still have: NaN or not, and two ends, each a value and whether it
is open; no end is none. -/
structure Interval (V : Type) where
  nan : Bool
  lo : Option (V × Bool)
  hi : Option (V × Bool)

namespace Num

variable {V : Type} (N : Num V)

theorem flip {a b : V} (h : N.le a b = false) : N.le b a = true := by
  rcases N.total a b with h' | h' <;> simp_all

/-- `x op c` of a number that may be NaN (`none`). -/
def cmp : Op → Option V → V → Bool
  | _, none, _ => false
  | .lt, some x, c => !N.le c x
  | .le, some x, c => N.le x c
  | .gt, some x, c => !N.le x c
  | .ge, some x, c => N.le c x

def above : Option (V × Bool) → V → Bool
  | none, _ => true
  | some (l, true), x => !N.le x l
  | some (l, false), x => N.le l x

def below : Option (V × Bool) → V → Bool
  | none, _ => true
  | some (h, true), x => !N.le h x
  | some (h, false), x => N.le x h

/-- Where the numbers that give `r` to `x op c` are: above `c` or below, and whether the end is
open. NaN gives `false` alone. -/
def half : Op → Bool → Bool × Bool
  | .gt, true => (true, true)
  | .ge, true => (true, false)
  | .lt, true => (false, true)
  | .le, true => (false, false)
  | .gt, false => (false, false)
  | .ge, false => (false, true)
  | .lt, false => (true, false)
  | .le, false => (true, true)

def onSide (s : Bool × Bool) (c x : V) : Bool :=
  if s.1 then N.above (some (c, s.2)) x else N.below (some (c, s.2)) x

theorem half_spec (op : Op) (r : Bool) (c x : V) :
    N.cmp op (some x) c = r ↔ N.onSide (half op r) c x = true := by
  cases op <;> cases r <;> simp [cmp, half, onSide, above, below]

def mem (I : Interval V) : Option V → Bool
  | none => I.nan
  | some x => N.above I.lo x && N.below I.hi x

/-- The tighter of two lower ends; either holds of what both hold of. -/
def tightenLo : Option (V × Bool) → V × Bool → Option (V × Bool)
  | none, e => some e
  | some (l, lo), (v, o) => if !N.le v l || (N.le l v && o) then some (v, o) else some (l, lo)

def tightenHi : Option (V × Bool) → V × Bool → Option (V × Bool)
  | none, e => some e
  | some (h, ho), (v, o) => if !N.le h v || (N.le v h && o) then some (v, o) else some (h, ho)

/-- Learns that `x op c` gave `r` (`Interval::learn`). -/
def learn (I : Interval V) (op : Op) (c : V) (r : Bool) : Interval V :=
  let s := half op r
  { nan := I.nan && !r
    lo := if s.1 then N.tightenLo I.lo (c, s.2) else I.lo
    hi := if s.1 then I.hi else N.tightenHi I.hi (c, s.2) }

theorem learn_sound (I : Interval V) (op : Op) (c : V) (r : Bool) (x : Option V)
    (hm : N.mem I x = true) (hc : N.cmp op x c = r) : N.mem (N.learn I op c r) x = true := by
  cases x with
  | none => simp [cmp] at hc; subst hc; simpa [mem, learn] using hm
  | some v =>
    have hs := (N.half_spec op r c v).1 hc
    simp only [mem, Bool.and_eq_true] at hm ⊢
    obtain ⟨hlo, hhi⟩ := hm
    simp only [learn]
    generalize half op r = s at hs ⊢
    obtain ⟨up, o⟩ := s
    cases up with
    | true =>
      simp only [onSide, ite_true] at hs
      refine ⟨?_, hhi⟩
      cases e : I.lo with
      | none => simpa [tightenLo] using hs
      | some l =>
        obtain ⟨l, lo⟩ := l
        rw [e] at hlo
        simp only [tightenLo]
        by_cases q : (!N.le c l || N.le l c && o) = true
        · rw [if_pos q]; exact hs
        · rw [if_neg q]; exact hlo
    | false =>
      simp only [onSide, Bool.false_eq_true, if_false] at hs
      refine ⟨hlo, ?_⟩
      cases e : I.hi with
      | none => simpa [tightenHi] using hs
      | some h =>
        obtain ⟨h, ho⟩ := h
        rw [e] at hhi
        simp only [tightenHi]
        by_cases q : (!N.le h c || N.le c h && o) = true
        · rw [if_pos q]; exact hs
        · rw [if_neg q]; exact hhi

/-- Every number above `e` is above `(c, o)`. -/
def loImplies : Option (V × Bool) → V × Bool → Bool
  | none, _ => false
  | some (l, lo), (c, o) => !N.le l c || (N.le c l && (!o || lo))

def hiImplies : Option (V × Bool) → V × Bool → Bool
  | none, _ => false
  | some (h, ho), (c, o) => !N.le c h || (N.le h c && (!o || ho))

/-- No number below `e` is above `(c, o)`. -/
def hiExcludes : Option (V × Bool) → V × Bool → Bool
  | none, _ => false
  | some (h, ho), (c, o) => !N.le c h || (N.le h c && (o || ho))

def loExcludes : Option (V × Bool) → V × Bool → Bool
  | none, _ => false
  | some (l, lo), (c, o) => !N.le l c || (N.le c l && (o || lo))

def empty (I : Interval V) : Bool :=
  match I.lo, I.hi with
  | some (l, lo), some (h, ho) => !N.le l h || (N.le h l && (lo || ho))
  | _, _ => false

/-- Every number of the interval is on the side `s` of `c`. -/
def within (I : Interval V) (s : Bool × Bool) (c : V) : Bool :=
  if s.1 then N.loImplies I.lo (c, s.2) else N.hiImplies I.hi (c, s.2)

/-- No number of the interval is on the side `s` of `c`. -/
def apart (I : Interval V) (s : Bool × Bool) (c : V) : Bool :=
  if s.1 then N.hiExcludes I.hi (c, s.2) else N.loExcludes I.lo (c, s.2)

/-- What `x op c` gives for every number the interval holds (`Interval::decide`). -/
def decide (I : Interval V) (op : Op) (c : V) : Option Bool :=
  if N.empty I then some !I.nan
  else if N.within I (half op true) c && !I.nan then some true
  else if N.apart I (half op true) c then some false
  else none

theorem loImplies_sound {e : Option (V × Bool)} {c x : V} {o : Bool}
    (hx : N.above e x = true) (h : N.loImplies e (c, o) = true) :
    N.above (some (c, o)) x = true := by
  cases e with
  | none => simp [loImplies] at h
  | some e =>
    obtain ⟨l, lo⟩ := e
    -- `x` is at least `l` either way
    have hlx : N.le l x = true := by
      cases lo with
      | true => simp [above] at hx; exact N.flip hx
      | false => simpa [above] using hx
    simp only [loImplies, Bool.or_eq_true, Bool.not_eq_true', Bool.and_eq_true] at h
    cases o with
    | false =>
      simp only [above]
      rcases h with h | ⟨hcl, _⟩
      · exact N.trans (N.flip h) hlx
      · exact N.trans hcl hlx
    | true =>
      simp only [above, Bool.not_eq_true']
      cases hxc : N.le x c with
      | false => rfl
      | true =>
        rcases h with h | ⟨hcl, hlo⟩
        · have := N.trans hlx hxc; simp_all
        · cases lo with
          | false => simp at hlo
          | true => simp [above] at hx; have := N.trans hxc hcl; simp_all

theorem hiImplies_sound {e : Option (V × Bool)} {c x : V} {o : Bool}
    (hx : N.below e x = true) (h : N.hiImplies e (c, o) = true) :
    N.below (some (c, o)) x = true := by
  cases e with
  | none => simp [hiImplies] at h
  | some e =>
    obtain ⟨hh, ho⟩ := e
    have hxh : N.le x hh = true := by
      cases ho with
      | true => simp [below] at hx; exact N.flip hx
      | false => simpa [below] using hx
    simp only [hiImplies, Bool.or_eq_true, Bool.not_eq_true', Bool.and_eq_true] at h
    cases o with
    | false =>
      simp only [below]
      rcases h with h | ⟨hhc, _⟩
      · exact N.trans hxh (N.flip h)
      · exact N.trans hxh hhc
    | true =>
      simp only [below, Bool.not_eq_true']
      cases hcx : N.le c x with
      | false => rfl
      | true =>
        rcases h with h | ⟨hhc, hho⟩
        · have := N.trans hcx hxh; simp_all
        · cases ho with
          | false => simp at hho
          | true => simp [below] at hx; have := N.trans hhc hcx; simp_all

theorem hiExcludes_sound {e : Option (V × Bool)} {c x : V} {o : Bool}
    (hx : N.below e x = true) (h : N.hiExcludes e (c, o) = true) :
    N.above (some (c, o)) x = false := by
  cases e with
  | none => simp [hiExcludes] at h
  | some e =>
    obtain ⟨hh, ho⟩ := e
    have hxh : N.le x hh = true := by
      cases ho with
      | true => simp [below] at hx; exact N.flip hx
      | false => simpa [below] using hx
    simp only [hiExcludes, Bool.or_eq_true, Bool.not_eq_true', Bool.and_eq_true] at h
    cases o with
    | true =>
      simp only [above, Bool.not_eq_false']
      rcases h with h | ⟨hhc, _⟩
      · exact N.trans hxh (N.flip h)
      · exact N.trans hxh hhc
    | false =>
      simp only [above]
      cases hcx : N.le c x with
      | false => rfl
      | true =>
        rcases h with h | ⟨hhc, hho⟩
        · have := N.trans hcx hxh; simp_all
        · cases ho with
          | false => simp at hho
          | true => simp [below] at hx; have := N.trans hhc hcx; simp_all

theorem loExcludes_sound {e : Option (V × Bool)} {c x : V} {o : Bool}
    (hx : N.above e x = true) (h : N.loExcludes e (c, o) = true) :
    N.below (some (c, o)) x = false := by
  cases e with
  | none => simp [loExcludes] at h
  | some e =>
    obtain ⟨l, lo⟩ := e
    have hlx : N.le l x = true := by
      cases lo with
      | true => simp [above] at hx; exact N.flip hx
      | false => simpa [above] using hx
    simp only [loExcludes, Bool.or_eq_true, Bool.not_eq_true', Bool.and_eq_true] at h
    cases o with
    | true =>
      simp only [below, Bool.not_eq_false']
      rcases h with h | ⟨hcl, _⟩
      · exact N.trans (N.flip h) hlx
      · exact N.trans hcl hlx
    | false =>
      simp only [below]
      cases hxc : N.le x c with
      | false => rfl
      | true =>
        rcases h with h | ⟨hcl, hlo⟩
        · have := N.trans hlx hxc; simp_all
        · cases lo with
          | false => simp at hlo
          | true => simp [above] at hx; have := N.trans hxc hcl; simp_all

theorem empty_sound {I : Interval V} {x : V} (he : N.empty I = true)
    (hm : N.mem I (some x) = true) : False := by
  simp only [mem, Bool.and_eq_true] at hm
  obtain ⟨hlo, hhi⟩ := hm
  unfold empty at he
  split at he
  · rename_i l lo hh ho elo ehi
    rw [elo] at hlo; rw [ehi] at hhi
    have hlx : N.le l x = true := by
      cases lo with
      | true => simp [above] at hlo; exact N.flip hlo
      | false => simpa [above] using hlo
    have hxh : N.le x hh = true := by
      cases ho with
      | true => simp [below] at hhi; exact N.flip hhi
      | false => simpa [below] using hhi
    have hlh := N.trans hlx hxh
    simp only [Bool.or_eq_true, Bool.not_eq_true', Bool.and_eq_true] at he
    rcases he with h | ⟨hhl, hopen⟩
    · simp_all
    · cases lo with
      | true =>
        simp [above] at hlo
        have := N.trans hxh hhl; simp_all
      | false =>
        cases ho with
        | true => simp [below] at hhi; have := N.trans hhl hlx; simp_all
        | false => simp at hopen
  · simp at he

/-- What `decide` gives, `x op c` gives of every number the interval holds. -/
theorem decide_sound {I : Interval V} {op : Op} {c : V} {b : Bool} (x : Option V)
    (hd : N.decide I op c = some b) (hm : N.mem I x = true) : N.cmp op x c = b := by
  have spec := N.half_spec op true c
  unfold decide at hd
  generalize hs : half op true = s at hd spec
  obtain ⟨up, o⟩ := s
  split at hd
  · rename_i he
    simp only [Option.some.injEq] at hd; subst hd
    cases x with
    | none => simp [mem] at hm; simp [cmp, hm]
    | some v => exact (N.empty_sound he hm).elim
  · split at hd
    · rename_i _ hw
      simp only [Option.some.injEq] at hd; subst hd
      simp only [within, Bool.and_eq_true, Bool.not_eq_true'] at hw
      cases x with
      | none => simp [mem] at hm; simp_all
      | some v =>
        apply (spec v).2
        simp only [mem, Bool.and_eq_true] at hm
        cases up with
        | true => simpa [onSide] using N.loImplies_sound hm.1 (by simpa using hw.1)
        | false => simpa [onSide] using N.hiImplies_sound hm.2 (by simpa using hw.1)
    · split at hd
      · rename_i _ _ ha
        simp only [Option.some.injEq] at hd; subst hd
        simp only [apart] at ha
        cases x with
        | none => simp [cmp]
        | some v =>
          cases hcv : N.cmp op (some v) c with
          | false => rfl
          | true =>
            have hside := (spec v).1 hcv
            simp only [mem, Bool.and_eq_true] at hm
            cases up with
            | true =>
              have := N.hiExcludes_sound hm.2 (by simpa using ha)
              simp_all [onSide]
            | false =>
              have := N.loExcludes_sound hm.1 (by simpa using ha)
              simp_all [onSide]
      · simp at hd

end Num

/-- The integers: an `Int` is never NaN, and the checker moves an open end of one to the closed
end next to it (`Interval::side`), which holds of the same integers. -/
def intNum : Num Int where
  le a b := decide (a ≤ b)
  total a b := by simp; omega
  trans := by intro a b c; simp; omega

theorem Interval.int_side (c x : Int) :
    intNum.above (some (c, true)) x = intNum.above (some (c + 1, false)) x ∧
      intNum.below (some (c, true)) x = intNum.below (some (c - 1, false)) x := by
  constructor
  · show (!decide (x ≤ c)) = decide (c + 1 ≤ x)
    by_cases h : x ≤ c
    · have : ¬ c + 1 ≤ x := by omega
      simp [h, this]
    · have : c + 1 ≤ x := by omega
      simp [h, this]
  · show (!decide (c ≤ x)) = decide (x ≤ c - 1)
    by_cases h : c ≤ x
    · have : ¬ x ≤ c - 1 := by omega
      simp [h, this]
    · have : x ≤ c - 1 := by omega
      simp [h, this]

/-! ## Atoms and numbers together

`Knowledge` of `logic.rs`: the atoms with their results, and an interval for each compared
subject. An atom known by its result gives it; one that compares a subject with a constant
gives what the subject's interval decides. -/

/-- How atoms compare numbers: the subject, the comparison and the constant of an atom that
compares, and every value a subject may have, before facts (`Interval::of`). -/
structure Numbers (α σ V : Type) where
  N : Num V
  cmpOf : α → Option (σ × Op × V)
  full : σ → Interval V

variable {σ V : Type}

/-- The interval of a subject: the last one learned, or its full one. -/
def Numbers.interval [DecidableEq σ] (P : Numbers α σ V) (l : List (σ × Interval V)) (s : σ) :
    Interval V :=
  ((l.find? (fun p => p.1 = s)).map Prod.snd).getD (P.full s)

def numbers [DecidableEq α] [DecidableEq σ] (P : Numbers α σ V) :
    Theory α (List (α × Bool) × List (σ × Interval V)) where
  known k a :=
    match lookup k.1 a with
    | some b => some b
    | none =>
      match P.cmpOf a with
      | some (s, op, c) => P.N.decide (P.interval k.2 s) op c
      | none => none
  add k a r :=
    if (lookup k.1 a).isSome then k
    else
      (k.1 ++ [(a, r)],
        match P.cmpOf a with
        | some (s, op, c) => (s, P.N.learn (P.interval k.2 s) op c r) :: k.2
        | none => k.2)

theorem Numbers.interval_sound [DecidableEq σ] (P : Numbers α σ V) {val : σ → Option V}
    (hfull : ∀ s, P.N.mem (P.full s) (val s) = true) {l : List (σ × Interval V)}
    (hl : ∀ s I, (s, I) ∈ l → P.N.mem I (val s) = true) (s : σ) :
    P.N.mem (P.interval l s) (val s) = true := by
  unfold interval
  cases hf : l.find? (fun p => p.1 = s) with
  | none => simpa using hfull s
  | some p =>
    have hm := List.mem_of_find?_eq_some hf
    have hp := List.find?_some hf
    obtain ⟨s', I⟩ := p
    simp at hp; subst hp
    simpa using hl _ _ hm

/-- **Soundness with numbers.** In a world whose subjects have the values `val`, each in its
full interval, where an atom that compares gives the comparison of its subject's value: what
the checker learns of atoms and intervals holds there, so `sound` applies to it. -/
theorem numbers_sound [DecidableEq α] [DecidableEq σ] (P : Numbers α σ V) {val : σ → Option V}
    (hfull : ∀ s, P.N.mem (P.full s) (val s) = true)
    (hcmp : ∀ a s op c, P.cmpOf a = some (s, op, c) → w.atom a = some (P.N.cmp op (val s) c)) :
    (numbers P).Sound w (fun k => (∀ a b, (a, b) ∈ k.1 → w.atom a = some b) ∧
      ∀ s I, (s, I) ∈ k.2 → P.N.mem I (val s) = true) where
  known k a b hk h := by
    simp only [numbers] at h
    cases hl : lookup k.1 a with
    | some c => rw [hl] at h; simp at h; subst h; exact lookup_sound hk.1 hl
    | none =>
      rw [hl] at h
      cases hc : P.cmpOf a with
      | none => rw [hc] at h; simp at h
      | some t =>
        obtain ⟨s, op, c⟩ := t
        rw [hc] at h
        rw [hcmp a s op c hc]
        congr 1
        exact P.N.decide_sound (val s) h (P.interval_sound hfull hk.2 s)
  add k a r hk he := by
    simp only [numbers]
    split
    · exact hk
    · refine ⟨?_, ?_⟩
      · intro a' b' hm
        simp at hm
        rcases hm with hm | ⟨rfl, rfl⟩
        · exact hk.1 _ _ hm
        · exact he
      · cases hc : P.cmpOf a with
        | none => simpa using hk.2
        | some t =>
          obtain ⟨s, op, c⟩ := t
          intro s' I hm
          simp at hm
          rcases hm with ⟨rfl, rfl⟩ | hm
          · apply P.N.learn_sound _ _ _ _ _ (P.interval_sound hfull hk.2 s')
            have := hcmp a s' op c hc
            rw [this] at he
            simpa using he
          · exact hk.2 _ _ hm

end Flick.Refine
