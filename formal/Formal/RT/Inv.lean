import Formal.RT.Hyp

/-!
# λRT: invariants of reachable configurations (runtime.md §6)

The journal `J` is read through `Hist Good J`: every event satisfied `Good` with respect to
the events before it. `Inv` collects what holds of every reachable configuration; R1–R4
(`Props.lean`) are read off it.
-/

namespace Flick.RT

variable {T V : Type}

/-- The fresh ids an event mentions (owners, fibers, finalizers, Promises, maps). -/
def Ev.ids : Ev V → List Nat
  | .spawn π k w => [π, k, w]
  | .gkid π k | .branch π k => [π, k]
  | .reg _ i => [i]
  | .fin π i | .now π i | .run π i => [π, i]
  | .close π | .fins π | .closed π => [π]
  | .done k _ | .sup k _ => [k]
  | .ext π p => [π, p]
  | .settle p => [p]
  | .build m _ σ | .release m _ σ => [m, σ]
  | .obs π m _ σ i => [π, m, σ, i]
  | _ => []

/-- The finalizer `i` has started: by (C5) or by (Fin-Now). -/
def Started (i : Nat) (J : List (Ev V)) : Prop := ∃ π, .run π i ∈ J ∨ .now π i ∈ J

/-- The fiber `k` has ended. -/
def DoneJ (k : Nat) (J : List (Ev V)) : Prop := ∃ x, .done k x ∈ J

/-- The fiber `k` started in `π`: a child, a watcher, a group's child, a branch. -/
def StartedIn (π k : Nat) (J : List (Ev V)) : Prop :=
  (∃ w, .spawn π k w ∈ J) ∨ (∃ c, .spawn π c k ∈ J) ∨ .gkid π k ∈ J ∨ .branch π k ∈ J

/-- Everything started in `π` has ended, and every Promise of `π` has settled. -/
def KidsDone (π : Nat) (J : List (Ev V)) : Prop :=
  (∀ k, StartedIn π k J → DoneJ k J) ∧ ∀ p, .ext π p ∈ J → .settle p ∈ J

/-- Each event is good with respect to the events before it. -/
def Hist (Good : Ev V → List (Ev V) → Prop) (J : List (Ev V)) : Prop :=
  ∀ J₁ e J₂, J = J₁ ++ e :: J₂ → Good e J₂

/-- What each event of the closing contract requires of the past: И1, И2, R1, R2. -/
def Good : Ev V → List (Ev V) → Prop
  | .run π i, J => .fin π i ∈ J ∧ ¬ Started i J ∧ (∀ j, .run π j ∈ J → i < j) ∧
      .fins π ∈ J ∧ .closed π ∉ J ∧ KidsDone π J
  | .now _ i, J => ¬ Started i J
  | .fin π i, J => .fins π ∉ J ∧ ¬ Started i J ∧ (∀ π', .fin π' i ∉ J) ∧
      ∀ π' i', .fin π' i' ∈ J → i' < i
  | .close π, J => .close π ∉ J
  | .fins π, J => .close π ∈ J ∧ .fins π ∉ J ∧ KidsDone π J
  | .closed π, J => .fins π ∈ J ∧ .closed π ∉ J ∧ ∀ i, .fin π i ∈ J → .run π i ∈ J
  | .spawn π _ _, J => .close π ∉ J
  | .gkid π _, J => .fins π ∉ J
  | .branch π _, J => .fins π ∉ J
  | _, _ => True

/-- The owner a frame holds: the body of `scope`/`provide`, the build of a layer. -/
def Frame.held : Frame T V → Option Nat
  | .inside π _ => some π
  | .build _ _ _ σ _ _ => some σ
  | _ => none

/-- A frame of the fiber `j` is consistent with the owners. -/
def FrOk (C : Conf T V) (j : Nat) (fr : Frame T V) : Prop :=
  (∀ π env, fr = .inside π env → π ≠ 0 ∧ ∃ o, C.Λ π = some o ∧ o.st = .open ∧ o.hold = j) ∧
  (∀ πp m L σ Ls b, fr = .build πp m L σ Ls b → σ ≠ 0 ∧
    (∃ o, C.Λ σ = some o ∧ o.st = .open ∧ o.hold = j) ∧
    ∃ mm, C.M m = some mm ∧ mm.ents L = some ⟨.building, σ, []⟩) ∧
  (∀ π, fr = .closing π → π ≠ 0 ∧ ∃ o, C.Λ π = some o ∧ o.st ≠ .open)

theorem FrOk.inside {C : Conf T V} {j π : Nat} {env : Option Nat}
    (h : FrOk C j (.inside π env)) :
    π ≠ 0 ∧ ∃ o, C.Λ π = some o ∧ o.st = .open ∧ o.hold = j := h.1 π env rfl

theorem FrOk.build {C : Conf T V} {j πp m L σ : Nat} {Ls : List (Nat × T)} {b : T}
    (h : FrOk C j (.build πp m L σ Ls b)) : σ ≠ 0 ∧
      (∃ o, C.Λ σ = some o ∧ o.st = .open ∧ o.hold = j) ∧
      ∃ mm, C.M m = some mm ∧ mm.ents L = some ⟨.building, σ, []⟩ := h.2.1 πp m L σ Ls b rfl

theorem FrOk.closing {C : Conf T V} {j π : Nat} (h : FrOk C j (.closing π)) :
    π ≠ 0 ∧ ∃ o, C.Λ π = some o ∧ o.st ≠ .open := h.2.2 π rfl

/-- A `par` frame of the stack lists the branch `j`, and the frames under it make `π` the
current owner. -/
def ParHas (j π base : Nat) : List (Frame T V) → Prop
  | [] => False
  | .par ks :: b => (j ∈ ks ∧ curOf base b = π) ∨ ParHas j π base b
  | _ :: b => ParHas j π base b

/-- Who waits for a running fiber (the sketch of R3). -/
def Guard (C : Conf T V) (j : Nat) (φ : Fiber T V) : Prop :=
  (φ.role = .main → φ.base = 0) ∧
  (φ.role = .child → ∃ o, C.Λ φ.base = some o ∧ j ∈ o.K) ∧
  (φ.role = .watcher → ∃ o, C.Λ φ.base = some o ∧ j ∈ o.W) ∧
  (∀ γ, φ.role = .gkid γ → ∃ g, C.G γ = some g ∧ g.owner = φ.base ∧ j ∈ g.kids) ∧
  (∀ f, φ.role = .branch f → f < j ∧ ∃ ψ, C.Φ f = some (.run ψ) ∧ ParHas j φ.base ψ.base ψ.stack)

/-- An open group has its hook in a live owner; a closed one has ended its children, or its
owner's closing is waiting for them. -/
def GroupOk (C : Conf T V) (γ : Nat) (g : Grp) : Prop :=
  (g.isOpen = true → ∃ o, C.Λ g.owner = some o ∧ o.st.live = true ∧ .stop γ ∈ o.H) ∧
  (g.isOpen = false → (∀ k ∈ g.kids, Done C.Φ k) ∨
    ∃ o b, C.Λ g.owner = some o ∧ o.st = .closing ∧ o.ph = .stop γ b)

/-- The invariant of reachable configurations. -/
structure Inv (C : Conf T V) : Prop where
  fresh : ∀ k, C.n ≤ k → C.Φ k = none ∧ C.Λ k = none ∧ C.G k = none ∧ C.X k = none ∧
    C.M k = none
  evIds : ∀ e ∈ C.J, ∀ i ∈ e.ids, i < C.n
  hist : Hist Good C.J
  root : ∃ o, C.Λ 0 = some o ∧ o.st = .open
  /-- И1: the state of an owner is what its closing events say -/
  stJ : ∀ π o, C.Λ π = some o → (o.st = .open ↔ .close π ∉ C.J) ∧
    (o.st.live = true ↔ .fins π ∉ C.J) ∧ (o.st = .closed ↔ .closed π ∈ C.J)
  /-- И2: an owner past (C5) accepts nothing -/
  empty : ∀ π o, C.Λ π = some o → o.st.live = false →
    o.K = [] ∧ o.W = [] ∧ o.P = [] ∧ o.H = [] ∧ o.F = []
  /-- `F` before (C5), `fs` after: newest first, registered, not started -/
  finSorted : ∀ π o, C.Λ π = some o → (o.F ++ o.fs).Pairwise (fun a b => b.id < a.id)
  finIn : ∀ π o t, C.Λ π = some o → t ∈ o.F ++ o.fs → .fin π t.id ∈ C.J ∧
    ¬ Started t.id C.J ∧ ∀ j, .run π j ∈ C.J → t.id < j
  finAll : ∀ π o i, C.Λ π = some o → .fin π i ∈ C.J → (∃ t ∈ o.F ++ o.fs, t.id = i) ∨ .run π i ∈ C.J
  fsNil : ∀ π o, C.Λ π = some o → o.st ≠ .finalizing → o.fs = []
  /-- R1: a finalizer handed to an owner is in its stack or has run -/
  regJ : ∀ π i, .reg π i ∈ C.J → .fin π i ∈ C.J ∨ .now π i ∈ C.J
  /-- a running fiber started in a live owner -/
  live : ∀ j φ, C.Φ j = some (.run φ) → ∃ o, C.Λ φ.base = some o ∧ o.st.live = true
  guard : ∀ j φ, C.Φ j = some (.run φ) → Guard C j φ
  frames : ∀ j φ, C.Φ j = some (.run φ) → ∀ fr ∈ φ.stack, FrOk C j fr
  held : ∀ j φ, C.Φ j = some (.run φ) → (φ.stack.filterMap Frame.held).Nodup
  groups : ∀ γ g, C.G γ = some g → GroupOk C γ g
  /-- И3 -/
  spawnJ : ∀ π k w, .spawn π k w ∈ C.J →
    (∃ o, C.Λ π = some o ∧ (k ∈ o.K ∨ DoneJ k C.J) ∧ (w ∈ o.W ∨ DoneJ w C.J))
  startJ : ∀ π k, StartedIn π k C.J → ∃ s, C.Φ k = some s ∧ ∀ φ, s = .run φ → φ.base = π
  doneJ : ∀ k x, .done k x ∈ C.J ↔ C.Φ k = some (.done x)
  watchJ : ∀ π k w, .spawn π k w ∈ C.J →
    (∃ φ g, C.Φ w = some (.run φ) ∧ φ.code = .watch π k g) ∨
    (DoneJ k C.J ∧ ∀ c, .done k (.fail c) ∈ C.J → .sup k c ∈ C.J)
  extJ : ∀ π p, .ext π p ∈ C.J → .settle p ∈ C.J ∨
    (C.X p = some π ∧ ∃ o, C.Λ π = some o ∧ p ∈ o.P)
  defectJ : ∀ c, .defect c ∈ C.J → c ∈ C.D
  closeΛ : ∀ π, .close π ∈ C.J → ∃ o, C.Λ π = some o
  /-- a memo entry's area exists; once built, no frame holds it -/
  ents : ∀ m mm Ly e, C.M m = some mm → mm.ents Ly = some e → e.σ ≠ 0 ∧
    (∃ oσ, C.Λ e.σ = some oσ) ∧
    (e.st = .built → ∀ j φ, C.Φ j = some (.run φ) → ∀ fr ∈ φ.stack, fr.held ≠ some e.σ)

theorem hist_cons {G : Ev V → List (Ev V) → Prop} {e : Ev V} {J : List (Ev V)} :
    Hist G (e :: J) ↔ G e J ∧ Hist G J := by
  constructor
  · intro h
    refine ⟨h [] e J rfl, fun J₁ e' J₂ hJ => h (e :: J₁) e' J₂ (by simp [hJ])⟩
  · rintro ⟨he, hJ⟩ J₁ e' J₂ h
    cases J₁ with
    | nil => cases h; exact he
    | cons a J₁ => cases h; exact hJ J₁ e' J₂ rfl

theorem hist_mem {G : Ev V → List (Ev V) → Prop} {e : Ev V} {J : List (Ev V)}
    (h : Hist G J) (he : e ∈ J) : ∃ J₂, G e J₂ ∧ ∀ e' ∈ J₂, e' ∈ J := by
  obtain ⟨J₁, J₂, rfl⟩ := List.mem_iff_append.1 he
  exact ⟨J₂, h J₁ e J₂ rfl, fun _ h' => List.mem_append.2 (.inr (List.mem_cons_of_mem _ h'))⟩

end Flick.RT
