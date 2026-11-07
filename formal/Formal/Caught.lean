import Formal.AscStep

/-!
# Every signal is under its handler

`e.Caught H` says that each `unw p ℓ` of `e` is under the frame of its handler: `H`
holds the handlers whose frames are around the place, a handler's body is at `H` and
its region `p`, and the region of a handler is not yet a handler around its `inside`
form. Two maps hold for the whole configuration what a handler was installed with:
`hs p` the handlers around the handler `p`, where what its signal holds runs, and
`cell p` its cell. So a signal passes no frame of its own handler without being
caught, a closing region is never the handler of a signal in its body, and no thread
is a signal at its top.
-/

namespace Flick

section
variable (hs : Nat → Nat → Prop) (cell : Nat → Nat)

def Tm.Caught : Tm → (Nat → Prop) → Prop
  | .var _, _ | .lit _, _ | .loc _, _ | .const _, _ | .ctor _, _ | .rnil, _ | .anil, _
  | .todo, _ | .cap _, _ | .own _, _ | .glob _, _ | .kont _ _, _ | .hole _, _ => True
  | .lam b, H | .fix b, H | .proj b _, H | .task b, H | .await b, H | .raise b, H
  | .scope b, H | .drec _ b, H | .hdone _ b, H => b.Caught H
  | .app a b, H | .let_ a b, H | .letrec a b, H | .rcons _ a b, H | .update a _ b, H
  | .match_ a b, H | .acons _ a b, H | .try_ a b, H | .handle a b, H
  | .hres _ _ a b, H => a.Caught H ∧ b.Caught H
  | .aguard _ g b r, H => g.Caught H ∧ b.Caught H ∧ r.Caught H
  | .hdl _ al r e, H => al.Caught H ∧ r.Caught H ∧ e.Caught H
  | .inside p e, H => ¬ H p ∧ e.Caught H
  | .hwith p ℓ _ al r e, H => hs p = H ∧ cell p = ℓ ∧ al.Caught H ∧ r.Caught H ∧
      e.Caught (fun x => H x ∨ x = p)
  | .hctl p ℓ _ _ _ al r a k, H => hs p = H ∧ cell p = ℓ ∧ al.Caught H ∧ r.Caught H ∧
      a.Caught H ∧ k.Caught (fun x => H x ∨ x = p)
  | .unw p ℓ a, H => H p ∧ cell p = ℓ ∧ a.Caught (hs p)

def Frame.Caught (H : Nat → Prop) : Frame → Prop
  | .appL a | .appR a | .let_ a | .match_ a | .rconsL _ a | .rconsR _ a | .updL _ a | .updR a _
  | .try_ a | .hresB _ _ a => a.Caught hs cell H
  | .proj _ | .await | .raise | .hdone _ => True
  | .inside p => ¬ H p
  | .hwith p ℓ _ al r => hs p = H ∧ cell p = ℓ ∧ al.Caught hs cell H ∧ r.Caught hs cell H
  | .hctlA p ℓ _ _ _ al r k => hs p = H ∧ cell p = ℓ ∧ al.Caught hs cell H ∧ r.Caught hs cell H ∧
      k.Caught hs cell (fun x => H x ∨ x = p)

/-- The contexts of a context, from the outside in. -/
def CaughtE (H : Nat → Prop) : List Frame → Prop
  | [] => True
  | F :: E => CaughtE H E ∧ F.Caught hs cell (holeC H E)
where
  /-- The handlers around the hole. -/
  holeC (H : Nat → Prop) : List Frame → Nat → Prop
    | [] => H
    | .hwith p .. :: E => fun x => holeC H E x ∨ x = p
    | _ :: E => holeC H E

end

/-- The handlers around the hole of a frame. -/
def Frame.holeH (H : Nat → Prop) : Frame → Nat → Prop
  | .hwith p .. => fun x => H x ∨ x = p
  | _ => H

variable {hs : Nat → Nat → Prop} {cell : Nat → Nat}

theorem Frame.caught_plug {F : Frame} {H : Nat → Prop} {e : Tm} :
    (F.plug e).Caught hs cell H ↔ F.Caught hs cell H ∧ e.Caught hs cell (F.holeH H) := by
  cases F <;> simp only [Frame.plug, Tm.Caught, Frame.Caught, Frame.holeH] <;> grind

theorem caught_plug {e : Tm} {H : Nat → Prop} : ∀ {E : List Frame},
    (plug E e).Caught hs cell H ↔ CaughtE hs cell H E ∧ e.Caught hs cell (CaughtE.holeC H E)
  | [] => by simp [plug, CaughtE, CaughtE.holeC]
  | F :: E => by
    simp only [plug, CaughtE]
    rw [caught_plug, Frame.caught_plug]
    cases F <;> simp only [CaughtE.holeC, Frame.holeH, and_assoc]

theorem Tm.Caught.of_noIns {e : Tm} (hn : e.NoIns) : ∀ H, e.Caught hs cell H := by
  induction e <;> intro H <;> simp_all [Tm.NoIns, Tm.Has, Tm.Caught]

/-- The handlers around the hole are those around the context and the handlers of
its frames. -/
theorem CaughtE.holeC_has {H : Nat → Prop} {x : Nat} {r : Tm} :
    ∀ {E : List Frame}, CaughtE.holeC H E x → H x ∨ (plug E r).Has (.rgn x)
  | [], h => .inl h
  | F :: E, h => by
    cases F <;> simp only [CaughtE.holeC] at h <;>
      first
      | exact (holeC_has (E := E) (r := Frame.plug _ r) h).imp_right id
      | (rcases h with h | rfl
         · exact (holeC_has (E := E) (r := Frame.plug _ r) h).imp_right id
         · exact .inr (plug_has E (by simp [Frame.plug, Tm.Has])))

/-- The maps agree on the regions a term names. -/
theorem Tm.Caught.ghost {hs' : Nat → Nat → Prop} {cell' : Nat → Nat} :
    ∀ {e : Tm} {H : Nat → Prop}, e.Caught hs cell H →
      (∀ p, e.Has (.rgn p) → hs' p = hs p ∧ cell' p = cell p) → e.Caught hs' cell' H := by
  intro e
  induction e <;> intro H h hg <;> simp only [Tm.Caught, Tm.Has] at h hg ⊢ <;> grind

end Flick
