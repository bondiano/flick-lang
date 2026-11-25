import Formal.RT.Config

/-!
# λRT: a lazy cell runs one attempt at a time (R9, lazy.md §5)

A lazy cell is shared by fibers. Its state is `pending e n`, `running π n W e` — the fiber `π`
runs attempt `n` of the initializer `e`, and the fibers `W` wait — or `done w`. Each transition
is one atomic step of one fiber, as `Ref.modify` is (R7): a force starts an attempt, waits, is a
defect, or gets the cached outcome; an attempt ends with an outcome, which is cached, or by the
interruption of its owner or the unwind of its handler, which resets the cell; a waiter may be
interrupted. `LReach` is every interleaving of these steps from `lazy e`, with its journal.

R9 is a property of this protocol (`R9`): `Step` and `Inv` of `Config.lean` gain nothing. A cycle
of forces through other fibers waits forever, as with `Deferred`; that is liveness, which R9 does
not speak about.
-/

namespace Flick.RT

variable {T V : Type}

/-- An exit that a lazy caches: a value, typed failures, defects — no interruption. -/
def Cached (w : Out V) : Prop := ∀ f, Reason.int f ∉ w.cause

/-- The state of a lazy cell. `n` counts the attempts made. -/
inductive LSt (T V : Type) where
  | pending (e : T) (n : Nat)
  | running (π n : Nat) (W : List Nat) (e : T)
  | done (w : Out V)

/-- The atomic steps of a lazy cell, each of one fiber `f`. -/
inductive LEv (V : Type) where
  /-- `f` forces the pending cell and runs attempt `n` -/
  | start (f n : Nat)
  /-- `f` forces the cell while another fiber runs an attempt, and waits -/
  | wait (f : Nat)
  /-- the owner forces the cell its attempt runs: a defect -/
  | self (f : Nat)
  /-- `f` forces the settled cell and gets its outcome -/
  | hit (f : Nat) (w : Out V)
  /-- attempt `n` of `f` ends with `w`, which the cell caches; the waiters `W` get it -/
  | settle (f n : Nat) (w : Out V) (W : List Nat)
  /-- the waiter `f` is interrupted -/
  | leave (f : Nat)
  /-- attempt `n` of `f` ends by the interruption of `f` or the unwind of its handler; the
  waiters `W` force again -/
  | reset (f n : Nat) (W : List Nat)

inductive LStep : LSt T V → LEv V → LSt T V → Prop
  | start {e : T} {n f : Nat} : LStep (.pending e n) (.start f (n + 1)) (.running f (n + 1) [] e)
  | wait {π n : Nat} {W : List Nat} {e : T} {f : Nat} : f ≠ π → f ∉ W →
      LStep (.running π n W e) (.wait f) (.running π n (W ++ [f]) e)
  | self {f n : Nat} {W : List Nat} {e : T} : LStep (.running f n W e) (.self f) (.running f n W e)
  | hit {w : Out V} {f : Nat} : LStep (.done w) (.hit f w) (.done w)
  | settle {f n : Nat} {W : List Nat} {e : T} {w : Out V} : Cached w →
      LStep (.running f n W e) (.settle f n w W) (.done w)
  | leave {π n : Nat} {W : List Nat} {e : T} {f : Nat} : f ∈ W →
      LStep (.running π n W e) (.leave f) (.running π n (W.erase f) e)
  | reset {f n : Nat} {W : List Nat} {e : T} : LStep (.running f n W e) (.reset f n W) (.pending e n)

/-- A cell made by `lazy e₀`, after the steps of the journal `J` (newest first). -/
inductive LReach (e₀ : T) : LSt T V → List (LEv V) → Prop
  | init : LReach e₀ (.pending e₀ 0) []
  | step {s s' : LSt T V} {J : List (LEv V)} {ev : LEv V} :
      LReach e₀ s J → LStep s ev s' → LReach e₀ s' (ev :: J)

/-- Steps from `s` to `s'` (oldest first). -/
inductive LRun : LSt T V → List (LEv V) → LSt T V → Prop
  | nil {s : LSt T V} : LRun s [] s
  | cons {s s₁ s' : LSt T V} {ev : LEv V} {J : List (LEv V)} :
      LStep s ev s₁ → LRun s₁ J s' → LRun s (ev :: J) s'

/-- What a step gives a fiber that forced the cell. -/
inductive Gets (V : Type) where
  | runs (n : Nat)
  | waits
  | defect
  | got (w : Out V)
  | retries
  | interrupted

def LEv.gets : LEv V → Nat → Option (Gets V)
  | .start f n, g => if g = f then some (.runs n) else none
  | .wait f, g => if g = f then some .waits else none
  | .self f, g => if g = f then some .defect else none
  | .hit f w, g => if g = f then some (.got w) else none
  | .settle f _ w W, g => if g = f ∨ g ∈ W then some (.got w) else none
  | .leave f, g => if g = f then some .interrupted else none
  | .reset f _ W, g => if g = f then some .interrupted else if g ∈ W then some .retries else none

/-- The step is a force by `f`. -/
def LEv.forces (f : Nat) : LEv V → Prop
  | .start g _ | .wait g | .self g | .hit g _ => g = f
  | _ => False

def LEv.starts (n : Nat) : LEv V → Prop
  | .start _ m => m = n
  | _ => False

def LEv.ends (n : Nat) : LEv V → Prop
  | .settle _ m _ _ | .reset _ m _ => m = n
  | _ => False

/-- Attempt `n` has started and not ended. -/
def Runs (J : List (LEv V)) (n : Nat) : Prop := (∃ ev ∈ J, ev.starts n) ∧ ∀ ev ∈ J, ¬ ev.ends n

/-- The attempts of a journal: those started and those ended, by the state of the cell; the
owner does not wait, and no fiber waits twice. -/
def LInv : LSt T V → List (LEv V) → Prop
  | .pending _ n, J => (∀ k, (∃ ev ∈ J, ev.starts k) ↔ 1 ≤ k ∧ k ≤ n) ∧
      ∀ k, (∃ ev ∈ J, ev.ends k) ↔ 1 ≤ k ∧ k ≤ n
  | .running π n W _, J => (∀ k, (∃ ev ∈ J, ev.starts k) ↔ 1 ≤ k ∧ k ≤ n) ∧
      (∀ k, (∃ ev ∈ J, ev.ends k) ↔ 1 ≤ k ∧ k < n) ∧ 1 ≤ n ∧ π ∉ W ∧ W.Nodup
  | .done _, J => ∀ k, (∃ ev ∈ J, ev.starts k) ↔ ∃ ev ∈ J, ev.ends k

theorem ex_cons {P : LEv V → Prop} {ev : LEv V} {J : List (LEv V)} :
    (∃ x ∈ ev :: J, P x) ↔ P ev ∨ ∃ x ∈ J, P x := by simp

theorem LInv.step {s s' : LSt T V} {J : List (LEv V)} {ev : LEv V} (hi : LInv s J)
    (hs : LStep s ev s') : LInv s' (ev :: J) := by
  cases hs with
  | start =>
    obtain ⟨h₁, h₂⟩ := hi
    refine ⟨fun k => ?_, fun k => ?_, by omega, by simp, .nil⟩
    · rw [ex_cons, h₁]; simp only [LEv.starts]; omega
    · rw [ex_cons, h₂]; simp only [LEv.ends, false_or]; omega
  | wait hπ hW =>
    obtain ⟨h₁, h₂, hn, hπW, hd⟩ := hi
    refine ⟨fun k => ?_, fun k => ?_, hn, by simp [hπW, Ne.symm hπ], ?_⟩
    · rw [ex_cons, h₁]; simp [LEv.starts]
    · rw [ex_cons, h₂]; simp [LEv.ends]
    · exact List.nodup_append.mpr ⟨hd, by simp, fun a ha b hb => by
        simp at hb; subst hb; exact fun h => hW (h ▸ ha)⟩
  | self =>
    obtain ⟨h₁, h₂, hn, hπW, hd⟩ := hi
    refine ⟨fun k => ?_, fun k => ?_, hn, hπW, hd⟩
    · rw [ex_cons, h₁]; simp [LEv.starts]
    · rw [ex_cons, h₂]; simp [LEv.ends]
  | hit =>
    intro k
    rw [ex_cons, ex_cons, hi k]; simp [LEv.starts, LEv.ends]
  | settle =>
    obtain ⟨h₁, h₂, hn, -, -⟩ := hi
    intro k
    rw [ex_cons, ex_cons, h₁, h₂]; simp only [LEv.starts, LEv.ends, false_or]; omega
  | leave hf =>
    obtain ⟨h₁, h₂, hn, hπW, hd⟩ := hi
    refine ⟨fun k => ?_, fun k => ?_, hn, fun h => hπW (List.mem_of_mem_erase h), hd.erase _⟩
    · rw [ex_cons, h₁]; simp [LEv.starts]
    · rw [ex_cons, h₂]; simp [LEv.ends]
  | reset =>
    obtain ⟨h₁, h₂, hn, -, -⟩ := hi
    refine ⟨fun k => ?_, fun k => ?_⟩
    · rw [ex_cons, h₁]; simp [LEv.starts]
    · rw [ex_cons, h₂]; simp only [LEv.ends]; omega

theorem LReach.inv {e₀ : T} {s : LSt T V} {J : List (LEv V)} (h : LReach e₀ s J) : LInv s J := by
  induction h with
  | init => exact ⟨fun k => by simp; omega, fun k => by simp; omega⟩
  | step _ hs ih => exact ih.step hs

/-- (a): the attempt that runs is the one of the running state. -/
theorem LInv.runs {s : LSt T V} {J : List (LEv V)} (hi : LInv s J) (n : Nat) :
    Runs J n ↔ ∃ π W e, s = .running π n W e := by
  cases s with
  | pending e m =>
    obtain ⟨h₁, h₂⟩ := hi
    simp only [Runs, reduceCtorEq, exists_false, iff_false, not_and]
    intro hs he
    exact ((h₂ n).2 ((h₁ n).1 hs)).elim fun ev ⟨hm, hev⟩ => he ev hm hev
  | running π m W e =>
    obtain ⟨h₁, h₂, hm₁, -, -⟩ := hi
    constructor
    · rintro ⟨hs, he⟩
      have := (h₁ n).1 hs
      have hne : ¬ (1 ≤ n ∧ n < m) := fun h => by
        obtain ⟨ev, hm, hev⟩ := (h₂ n).2 h; exact he ev hm hev
      have hnm : n = m := by omega
      subst hnm
      exact ⟨π, W, e, rfl⟩
    · rintro ⟨_, _, _, h⟩
      cases h
      refine ⟨(h₁ n).2 ⟨hm₁, Nat.le_refl _⟩, fun ev hm hev => ?_⟩
      have := (h₂ n).1 ⟨ev, hm, hev⟩; omega
  | done w =>
    simp only [Runs, reduceCtorEq, exists_false, iff_false, not_and]
    intro hs he
    obtain ⟨ev, hm, hev⟩ := (hi n).1 hs
    exact he ev hm hev

/-- (b): a settled cell stays settled, and every later step is a force that gets its outcome. -/
theorem R9b {w : Out V} : ∀ {J : List (LEv V)} {s' : LSt T V}, LRun (.done w) J s' →
    s' = .done w ∧ ∀ ev ∈ J, ∃ f, ev = .hit f w ∧ ev.gets f = some (.got w)
  | [], _, h => by cases h; exact ⟨rfl, by simp⟩
  | _ :: _, _, h => by
    cases h with
    | cons h₁ h₂ =>
      cases h₁
      obtain ⟨h₃, h₄⟩ := R9b h₂
      refine ⟨h₃, fun ev hev => ?_⟩
      rcases List.mem_cons.mp hev with rfl | hev
      · exact ⟨_, rfl, by simp [LEv.gets]⟩
      · exact h₄ ev hev

/-- (c): the interruption of a waiter removes only that waiter; the owner, the attempt and the
initializer stay. -/
theorem R9c {s s' : LSt T V} {J : List (LEv V)} {f : Nat} (hi : LInv s J)
    (hs : LStep s (.leave f) s') :
    ∃ π n W e, s = .running π n W e ∧ s' = .running π n (W.erase f) e ∧ f ∈ W ∧
      f ∉ W.erase f ∧ ∀ g, g ≠ f → (g ∈ W.erase f ↔ g ∈ W) := by
  cases hs with
  | leave hf =>
    obtain ⟨-, -, -, -, hd⟩ := hi
    exact ⟨_, _, _, _, rfl, rfl, hf, hd.not_mem_erase, fun g hg => List.mem_erase_of_ne hg⟩

/-- (d): the end of an attempt by an interruption of its owner or the unwind of its handler
caches nothing: the cell is pending again, the owner gets its interruption, the waiters force
again, and the first fiber to force runs the next attempt. Only an outcome without an
interruption is cached. -/
theorem R9d {s s' : LSt T V} {J : List (LEv V)} {f n : Nat} {W : List Nat} (hi : LInv s J)
    (hs : LStep s (.reset f n W) s') :
    ∃ e, s = .running f n W e ∧ s' = .pending e n ∧
      (LEv.reset f n W : LEv V).gets f = some .interrupted ∧
      (∀ g ∈ W, (LEv.reset f n W : LEv V).gets g = some .retries) ∧
      ∀ ev (s'' : LSt T V), LStep s' ev s'' → ∃ g, ev = .start g (n + 1) ∧ s'' = .running g (n + 1) [] e := by
  cases hs with
  | reset =>
    obtain ⟨-, -, -, hfW, -⟩ := hi
    refine ⟨_, rfl, rfl, by simp [LEv.gets], fun g hg => ?_, fun ev s'' h => ?_⟩
    · have : g ≠ f := fun h => hfW (h ▸ hg)
      simp [LEv.gets, this, hg]
    · cases h; exact ⟨_, rfl, rfl⟩

theorem R9_cached {s s' : LSt T V} {f n : Nat} {w : Out V} {W : List Nat}
    (hs : LStep s (.settle f n w W) s') : Cached w ∧ s' = .done w := by
  cases hs with
  | settle hw => exact ⟨hw, rfl⟩

/-- (e): a force by the owner of a running cell is a defect; a force by another fiber waits. -/
theorem R9e {π n : Nat} {W : List Nat} {e : T} {ev : LEv V} {s' : LSt T V} {f : Nat}
    (hs : LStep (.running π n W e) ev s') (hf : ev.forces f) :
    (f = π → ev = .self π ∧ ev.gets π = some .defect) ∧ (f ≠ π → ev = .wait f) := by
  cases hs with
  | wait hπ => cases hf; exact ⟨fun h => (hπ h).elim, fun _ => rfl⟩
  | self => cases hf; exact ⟨fun _ => ⟨rfl, by simp [LEv.gets]⟩, fun h => (h rfl).elim⟩
  | settle | leave | reset => cases hf

/-- Once a force got an outcome, the cell holds it: an outcome comes only from a settle, which
caches it, or from a hit of the settled cell. -/
theorem LReach.got {e₀ : T} {s : LSt T V} {J : List (LEv V)} (h : LReach e₀ s J) {ev : LEv V}
    {f : Nat} {w : Out V} (hev : ev ∈ J) (hg : ev.gets f = some (.got w)) : s = .done w := by
  induction h with
  | init => cases hev
  | step _ hs ih =>
    rcases List.mem_cons.mp hev with rfl | hev
    · cases hs <;> simp [LEv.gets] at hg
      all_goals first
        | rfl
        | (obtain ⟨-, rfl⟩ := hg; rfl)
        | (subst hg; rfl)
        | (split at hg
           · cases hg
           · split at hg <;> cases hg)
    · cases ih hev; cases hs; rfl

/-- **O2a, of the protocol** (lazy.md §6): once a force of a cell got an outcome, every later
step of the cell is a force, by any fiber, that gets the same outcome. -/
theorem R9_again {e₀ : T} {s s' : LSt T V} {J J' : List (LEv V)} (h : LReach e₀ s J)
    {ev : LEv V} {f : Nat} {w : Out V} (hev : ev ∈ J) (hg : ev.gets f = some (.got w))
    (hr : LRun s J' s') :
    s' = .done w ∧ ∀ ev' ∈ J', ∃ g, ev' = .hit g w ∧ ev'.gets g = some (.got w) := by
  cases h.got hev hg
  exact R9b hr

/-- **O3, of the protocol** (lazy.md §6): a cell that only the fiber `f` forces runs its attempts
in `f` and never has a waiter. No step waits or leaves, and every settle and reset hands its
outcome to no waiter, so the synchronization of R9 is never exercised: a cell without it takes
the same steps. -/
theorem R9_owner {e₀ : T} {s : LSt T V} {J : List (LEv V)} {f : Nat} (h : LReach e₀ s J)
    (hf : ∀ ev ∈ J, ∀ g, ev.forces g → g = f) :
    (∀ π n W e, s = .running π n W e → π = f ∧ W = []) ∧
      ∀ ev ∈ J, ∀ g, ev ≠ .wait g ∧ ev ≠ .leave g := by
  revert hf
  induction h with
  | init => exact fun _ => ⟨fun _ _ _ _ h => (nomatch h), fun _ h => (nomatch h)⟩
  | @step _ _ J₀ _ _ hs ih =>
    intro hf
    obtain ⟨ih₁, ih₂⟩ := ih fun ev' h' => hf ev' (List.mem_cons_of_mem _ h')
    have hev := hf _ List.mem_cons_self
    have rest : ∀ {ev : LEv V}, (∀ g, ev ≠ .wait g ∧ ev ≠ .leave g) →
        ∀ ev' ∈ ev :: J₀, ∀ g, ev' ≠ .wait g ∧ ev' ≠ .leave g := by
      intro ev hn ev' hm g
      rcases List.mem_cons.mp hm with rfl | hm
      · exact hn g
      · exact ih₂ ev' hm g
    cases hs with
    | start => exact ⟨fun π n W e h => by cases h; exact ⟨hev _ rfl, rfl⟩, rest fun _ => ⟨nofun, nofun⟩⟩
    | wait hπ _ => exact absurd ((ih₁ _ _ _ _ rfl).1 ▸ hev _ rfl) hπ
    | self => exact ⟨fun π n W e h => by cases h; exact ih₁ _ _ _ _ rfl, rest fun _ => ⟨nofun, nofun⟩⟩
    | hit | settle _ | reset => exact ⟨fun π n W e h => (nomatch h), rest fun _ => ⟨nofun, nofun⟩⟩
    | leave hW => cases (ih₁ _ _ _ _ rfl).2; cases hW

/-- **R9 (single flight).** In every interleaving of the steps of a lazy cell: (a) at most one
attempt runs, the one of the running state, and the attempts run one after another, numbered
from 1; (b) a settled cell stays settled and every later force gets its outcome, as did the
waiters of the attempt that settled it; (c) the interruption of a waiter removes only that
waiter; (d) the interruption of the owner, or the unwind of its handler, makes the cell pending
again, gives the owner its interruption and the waiters a retry, and the next attempt is `n + 1`;
an outcome is cached only without an interruption; (e) a force by the owner of a running cell is
a defect, and a force by another fiber waits. -/
theorem R9 {e₀ : T} {s : LSt T V} {J : List (LEv V)} (h : LReach e₀ s J) :
    (∀ n m, Runs J n → Runs J m → n = m) ∧ (∀ n, Runs J n ↔ ∃ π W e, s = .running π n W e) ∧
    (∀ w J' (s' : LSt T V), s = .done w → LRun s J' s' →
      s' = .done w ∧ ∀ ev ∈ J', ∃ f, ev = .hit f w ∧ ev.gets f = some (.got w)) ∧
    (∀ f n w W (s' : LSt T V), LStep s (.settle f n w W) s' →
      Cached w ∧ s' = .done w ∧ ∀ g ∈ W, (LEv.settle f n w W).gets g = some (.got w)) ∧
    (∀ f (s' : LSt T V), LStep s (.leave f) s' → ∃ π n W e, s = .running π n W e ∧
      s' = .running π n (W.erase f) e ∧ f ∈ W ∧ f ∉ W.erase f ∧
      ∀ g, g ≠ f → (g ∈ W.erase f ↔ g ∈ W)) ∧
    (∀ f n W (s' : LSt T V), LStep s (.reset f n W) s' → ∃ e, s = .running f n W e ∧
      s' = .pending e n ∧ (LEv.reset f n W : LEv V).gets f = some .interrupted ∧
      (∀ g ∈ W, (LEv.reset f n W : LEv V).gets g = some .retries) ∧
      ∀ ev (s'' : LSt T V), LStep s' ev s'' →
        ∃ g, ev = .start g (n + 1) ∧ s'' = .running g (n + 1) [] e) ∧
    (∀ π n W e ev f (s' : LSt T V), s = .running π n W e → LStep s ev s' → ev.forces f →
      (f = π → ev = .self π ∧ ev.gets π = some .defect) ∧ (f ≠ π → ev = .wait f)) := by
  have hi := h.inv
  refine ⟨fun n m hn hm => ?_, hi.runs, fun w J' s' hs hr => by subst hs; exact R9b hr,
    fun f n w W s' hs => ?_, fun f s' hs => R9c hi hs, fun f n W s' hs => R9d hi hs,
    fun π n W e ev f s' hs h₁ hf => by subst hs; exact R9e h₁ hf⟩
  · obtain ⟨_, _, _, h₁⟩ := (hi.runs n).1 hn
    obtain ⟨_, _, _, h₂⟩ := (hi.runs m).1 hm
    rw [h₁] at h₂; cases h₂; rfl
  · obtain ⟨hw, rfl⟩ := R9_cached hs
    exact ⟨hw, rfl, fun g hg => by simp [LEv.gets, hg]⟩

end Flick.RT
