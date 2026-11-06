import Formal.Typing

/-!
# Evaluation contexts of λFlick (core.md §3.2)

A context is a list of frames, the innermost first; `plug E e` is `E[e]`. A handler
parks a body or an arm as the frames between it and its hole.
-/

namespace Flick

/-- A finished computation: a value, `raise v`, or unwinding to a handler. -/
def Final (e : Tm) : Prop := Value e ∨ (∃ v, Value v ∧ e = .raise v) ∨ ∃ p ℓ a, e = .unw p ℓ a

/-- One-frame evaluation contexts `F` of core.md §3.2. -/
inductive Frame
  | appL (e : Tm)
  | appR (v : Tm)
  | let_ (b : Tm)
  | match_ (al : Tm)
  | rconsL (l : Label) (r : Tm)
  | rconsR (l : Label) (v : Tm)
  | proj (l : Label)
  | updL (l : Label) (e' : Tm)
  | updR (v : Tm) (l : Label)
  | await
  | try_ (al : Tm)
  | provide (e : Tm)
  | bind (S : Name)
  | joinL (e : Tm)
  | joinR (v : Tm)
  | with_ (q : Nat) (θ : Tm)
  | raise
  | inside (p : Nat)
  | hwith (p ℓ : Nat) (E : Name) (al r : Tm)
  | hctlA (p ℓ κ : Nat) (tl : Bool) (E : Name) (al r k : Tm)
  | hresB (p ℓ : Nat) (a : Tm)
  | hdone (p : Nat)

def Frame.plug : Frame → Tm → Tm
  | .appL a, e => .app e a
  | .appR v, e => .app v e
  | .let_ b, e => .let_ e b
  | .match_ al, e => .match_ e al
  | .rconsL l r, e => .rcons l e r
  | .rconsR l v, e => .rcons l v e
  | .proj l, e => .proj e l
  | .updL l e', e => .update e l e'
  | .updR v l, e => .update v l e
  | .await, e => .await e
  | .try_ al, e => .try_ e al
  | .provide b, e => .provide e b
  | .bind S, e => .bind S e
  | .joinL b, e => .join e b
  | .joinR v, e => .join v e
  | .with_ q θ, e => .with_ q θ e
  | .raise, e => .raise e
  | .inside p, e => .inside p e
  | .hwith p ℓ E al r, e => .hwith p ℓ E al r e
  | .hctlA p ℓ κ tl E al r k, e => .hctl p ℓ κ tl E al r e k
  | .hresB p ℓ a, e => .hres p ℓ a e
  | .hdone p, e => .hdone p e

/-- Left-to-right order: a frame to the right of a value needs the value. -/
def Frame.ok : Frame → Prop
  | .appR v | .rconsR _ v | .updR v _ | .joinR v | .with_ _ v => Value v
  | _ => True

def Frame.isTry : Frame → Prop
  | .try_ _ => True
  | _ => False

/-- `raise v` does not leave a region: the region closes first (regions.md §3.3). -/
def Frame.isInside : Frame → Prop
  | .inside _ => True
  | _ => False

/-- The arm of a handler and its resumed body: `raise` and `unw` leave them by steps
of their own, which let the waiting part go. -/
def Frame.isCtl : Frame → Prop
  | .hctlA .. | .hresB .. => True
  | _ => False

/-- The handler that catches unwinding to `p` through its cell `ℓ`. -/
def Frame.catches (p ℓ : Nat) : Frame → Prop
  | .hwith q ℓ' .. => q = p ∧ ℓ' = ℓ
  | _ => False

/-- A frame where `resume` of the handler `p` stops: its arm, or the arm it resumed. -/
def Frame.resumes (p : Nat) : Frame → Prop
  | .hctlA q .. | .hdone q => q = p
  | _ => False

/-- `resume` of the cell `ℓ` resumes the body at the arm of a `control` operation that
holds it: the nearest frame where `resume` of `p` stops is that arm. -/
def Resumable (E : List Frame) (p ℓ : Nat) : Prop :=
  ∃ A E' ℓh Ef al r k, E = A ++ .hctlA p ℓh ℓ false Ef al r k :: E' ∧ ∀ G ∈ A, ¬ G.resumes p

/-- The handler of `E` with an arm for `o` that a frame installs, with its region. -/
def Frame.handles (E o : Name) : Frame → Option Nat
  | .hwith p _ E' al _ => if E' = E ∧ (al.armOf o).isSome then some p else none
  | _ => none

/-- The effect a handler frame handles in its body. -/
def Frame.installs : Frame → Name → Prop
  | .hwith _ _ E _ _, N => E = N
  | _, _ => False

/-- The service a `with` frame provides, with the region of its `provide`. -/
def Frame.lookup : Frame → Name → Option (Nat × Tm)
  | .with_ q θ, S => (θ.envLookup S).map (q, ·)
  | _, _ => none

/-- A `with` frame of a context kept around a child or a finalizer it registers. -/
def Frame.wrap : Frame → Tm → Tm
  | .with_ q θ, e => .with_ q θ e
  | _, e => e

/-- `E[e]`; the list starts at the innermost frame. -/
def plug : List Frame → Tm → Tm
  | [], e => e
  | F :: E, e => plug E (F.plug e)

def OkE (E : List Frame) : Prop := ∀ F ∈ E, Frame.ok F

/-- What a handler parks is a body or an arm at its hole. -/
def Frame.Parks : Frame → Prop
  | .hctlA p _ _ _ _ _ _ k => ∃ Ei, OkE Ei ∧ k = Flick.plug Ei (.hole p)
  | .hresB p _ a => ∃ A, OkE A ∧ a = Flick.plug A (.hole p)
  | _ => True

/-- `lookup(E, S)` of core.md §3.2 without the fallback to `Σ₀`. -/
def lookupE : List Frame → Name → Option (Nat × Tm)
  | [], _ => none
  | F :: E, S => match F.lookup S with
    | some v => some v
    | none => lookupE E S

/-- The `with` frames of a context around a term: the environment a child or a
finalizer inherits from where it is registered (runtime.md §3.5). -/
def withs : List Frame → Tm → Tm
  | [], e => e
  | F :: E, e => withs E (F.wrap e)

/-- What a term writes, a frame and a context around it write. -/
theorem Frame.plug_has {e : Tm} {s : Site} (F : Frame) (h : e.Has s) : (F.plug e).Has s := by
  cases F <;> simp only [Frame.plug, Tm.Has] <;> simp [h]

/-- A term in a context has everything its context gives it. -/
theorem plug_has {e : Tm} {s : Site} : ∀ (E : List Frame), e.Has s → (plug E e).Has s
  | [], h => h
  | F :: E, h => plug_has E (F.plug_has h)

theorem Frame.plug_has_inv {r r' : Tm} {s : Site} (F : Frame) (h : (F.plug r').Has s) :
    (F.plug r).Has s ∨ r'.Has s := by
  cases F <;> simp only [Frame.plug, Tm.Has] at h ⊢ <;> grind

theorem plug_has_inv {r r' : Tm} {s : Site} :
    ∀ (E : List Frame), (plug E r').Has s → (plug E r).Has s ∨ r'.Has s
  | [], h => .inr h
  | F :: E, h => by
    rcases plug_has_inv (r := F.plug r) E h with h | h
    · exact .inl h
    · rcases F.plug_has_inv (r := r) h with h | h
      · exact .inl (plug_has E h)
      · exact .inr h

/-- A term with the sites of a context and of the term in its hole: one replaced by
another with fewer sites. -/
theorem plug_has_iff {E : List Frame} {x : Tm} {s : Site} :
    (plug E x).Has s ↔ (plug E (.var 0)).Has s ∨ x.Has s :=
  ⟨fun h => plug_has_inv E h, fun h => h.elim (fun h => (plug_has_inv (r := x) E h).elim id
    (fun h => by simp [Tm.Has] at h)) (plug_has E)⟩

/-- A context around a term without sites has the sites of the context. -/
theorem plug_has_of_nosite {E : List Frame} {x y : Tm} {s : Site} (hx : ∀ s, ¬ x.Has s)
    (h : (plug E x).Has s) : (plug E y).Has s :=
  (plug_has_iff.mp h).elim (fun h => plug_has_iff.mpr (.inl h)) (fun h => (hx _ h).elim)

/-- No frame binds over its hole. -/
theorem plug_free_mono {a b : Tm} (hab : ∀ j, a.Free j → b.Free j) :
    ∀ {E : List Frame} {i : Nat}, (plug E a).Free i → (plug E b).Free i
  | [], _, h => hab _ h
  | F :: E, _, h => plug_free_mono (a := F.plug a) (b := F.plug b)
      (fun j h => by cases F <;> simp only [Frame.plug, Tm.Free] at h ⊢ <;> grind) (E := E) h

/-- A hole has no free variables. -/
theorem plug_free_of_hole {x : Tm} {p : Nat} {E : List Frame} {i : Nat} (h : (plug E (.hole p)).Free i) :
    (plug E x).Free i :=
  plug_free_mono (fun _ h => by simp [Tm.Free] at h) h

/-- A handler of `E` with arms `al` has an arm for every operation of `E`: `E` leaves the
row of its body. -/
def Sig.Complete (S : Sig) (E : Name) (al : Tm) : Prop :=
  ∀ o c a b, S.op E o = some (c, a, b) → (al.armOf o).isSome

/-- A handler handles a declared effect, which leaves the row `r` outside unless the
handler has an arm for every operation. -/
def Sig.Handles (S : Sig) (E : Name) (al : Tm) (r : Row) : Prop :=
  (∃ o c a b, S.op E o = some (c, a, b)) ∧ (¬ S.Complete E al → r (.nm E))

end Flick
