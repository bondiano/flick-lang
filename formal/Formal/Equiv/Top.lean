import Formal.Elab.CoreSim
import Formal.SubstArm

/-!
# Contextual equivalence: lazy cells where a pattern reads

The rewrites of O1 make or force a lazy cell at their top (`Tm.LzShape`). Where a pattern reads
a term without running it (`Pos.top` of `Formal/Equiv/Sim.lean`) a pattern could tell the two
sides of a rewrite apart, so there they are no pair of `Sim`. `Tm.LzTop x e`: such a term stands
in `e`, at `x`, at one of those places, outside the bodies of alternatives. Typing keeps it out
of a thread (`Formal/Equiv/TopTyped.lean`): those places have the pure stage, or no term
judgment, while a term that makes a cell has the stage `rt` at least. An arm whose body has one
is dead, as typing reaches every body but a dead arm of a GADT: `Sim.aconsD` relates it whatever
the bodies, and no run takes it.
-/

namespace Flick.Equiv

/-- Where a term stands. `top`: the spine of a dictionary record, where no special pair stands at
the top, as a pattern reads there without evaluating (the `true` of `Formal/Elab/CoreCg.lean`),
and a list of alternatives, which a match and a handler read.
`park`: the spine of a body or an arm parked at its hole, where both sides have the same frames
around the hole, up to the frames of cells only one side has, and the values of the frames are
related by `top`, so the right one is a value too. -/
inductive Pos
  | any
  | top
  | park
  deriving DecidableEq

/-- The position of the part of a frame that is evaluated in it. -/
@[reducible] def Pos.fr : Pos → Pos
  | .park => .park
  | _ => .any

/-- A term that makes or forces a lazy cell at its top. -/
def _root_.Flick.Tm.LzShape : Tm → Prop
  | .lazy _ => True
  | .let_ (.lazy _) _ => True
  | .force _ => True
  | .let_ (.force _) _ => True
  | _ => False

/-- `e` has a term that makes a lazy cell at a place a pattern reads, outside the bodies of
alternatives; `x` is where `e` stands. -/
inductive _root_.Flick.Tm.LzTop : Pos → Tm → Prop
  | here {e} : e.LzShape → Tm.LzTop .top e
  | lam {x b} : Tm.LzTop .any b → Tm.LzTop x (.lam b)
  | fix {x b} : Tm.LzTop .any b → Tm.LzTop x (.fix b)
  | task {x b} : Tm.LzTop .any b → Tm.LzTop x (.task b)
  | await {x b} : Tm.LzTop .any b → Tm.LzTop x (.await b)
  | lazy {x b} : Tm.LzTop .any b → Tm.LzTop x (.lazy b)
  | force {x b} : Tm.LzTop .any b → Tm.LzTop x (.force b)
  | raise {x b} : Tm.LzTop .any b → Tm.LzTop x (.raise b)
  | scope {x b} : Tm.LzTop .any b → Tm.LzTop x (.scope b)
  | proj {x b l} : Tm.LzTop .any b → Tm.LzTop x (.proj b l)
  | drec {x c r} : Tm.LzTop .top r → Tm.LzTop x (.drec c r)
  | appL {x f a} : Tm.LzTop x f → Tm.LzTop x (.app f a)
  | appR {x f a} : Tm.LzTop x a → Tm.LzTop x (.app f a)
  | letL {x a b} : Tm.LzTop .any a → Tm.LzTop x (.let_ a b)
  | letR {x a b} : Tm.LzTop .any b → Tm.LzTop x (.let_ a b)
  | letrecL {x a b} : Tm.LzTop .any a → Tm.LzTop x (.letrec a b)
  | letrecR {x a b} : Tm.LzTop .any b → Tm.LzTop x (.letrec a b)
  | rconsL {x l a r} : Tm.LzTop x a → Tm.LzTop x (.rcons l a r)
  | rconsR {x l a r} : Tm.LzTop x r → Tm.LzTop x (.rcons l a r)
  | updL {x a l b} : Tm.LzTop .any a → Tm.LzTop x (.update a l b)
  | updR {x a l b} : Tm.LzTop .any b → Tm.LzTop x (.update a l b)
  | matchL {x a al} : Tm.LzTop .any a → Tm.LzTop x (.match_ a al)
  | matchR {x a al} : Tm.LzTop .top al → Tm.LzTop x (.match_ a al)
  | acons {x p b r} : Tm.LzTop x r → Tm.LzTop x (.acons p b r)
  | aguardG {x p g b r} : Tm.LzTop .any g → Tm.LzTop x (.aguard p g b r)
  | aguardB {x p g b r} : Tm.LzTop .any b → Tm.LzTop x (.aguard p g b r)
  | aguardR {x p g b r} : Tm.LzTop x r → Tm.LzTop x (.aguard p g b r)
  | tryL {x a al} : Tm.LzTop .any a → Tm.LzTop x (.try_ a al)
  | tryR {x a al} : Tm.LzTop .top al → Tm.LzTop x (.try_ a al)
  | handleV {x v al} : Tm.LzTop .top v → Tm.LzTop x (.handle v al)
  | handleA {x v al} : Tm.LzTop .top al → Tm.LzTop x (.handle v al)
  | hdlA {x E al r b} : Tm.LzTop .top al → Tm.LzTop x (.hdl E al r b)
  | hdlR {x E al r b} : Tm.LzTop .any r → Tm.LzTop x (.hdl E al r b)
  | hdlE {x E al r b} : Tm.LzTop .any b → Tm.LzTop x (.hdl E al r b)
  | inside {x p b} : Tm.LzTop .any b → Tm.LzTop x (.inside p b)
  | hwith {x p ℓ E al r b} : Tm.LzTop .any b → Tm.LzTop x (.hwith p ℓ E al r b)
  | hkeep {x q ℓ E al b} : Tm.LzTop .any b → Tm.LzTop x (.hkeep q ℓ E al b)
  | hkarm {x q ℓ E al a k} : Tm.LzTop .any a → Tm.LzTop x (.hkarm q ℓ E al a k)
  | hctl {x p ℓ κ tl E al r a k} : Tm.LzTop .any a → Tm.LzTop x (.hctl p ℓ κ tl E al r a k)
  | hres {x p ℓ a b} : Tm.LzTop .any b → Tm.LzTop x (.hres p ℓ a b)
  | hdone {x p a} : Tm.LzTop .any a → Tm.LzTop x (.hdone p a)
  | unw {x p ℓ a} : Tm.LzTop .any a → Tm.LzTop x (.unw p ℓ a)
  | settle {x ℓ b} : Tm.LzTop .any b → Tm.LzTop x (.settle ℓ b)

theorem _root_.Flick.Tm.LzShape.rename {e : Tm} (h : e.LzShape) (ξ : Nat → Nat) :
    (e.rename ξ).LzShape := by
  match e, h with
  | .lazy _, _ => trivial
  | .let_ (.lazy _) _, _ => trivial
  | .force _, _ => trivial
  | .let_ (.force _) _, _ => trivial

theorem _root_.Flick.Tm.LzShape.subst {e : Tm} (h : e.LzShape) (σ : Nat → Tm) :
    (e.subst σ).LzShape := by
  match e, h with
  | .lazy _, _ => trivial
  | .let_ (.lazy _) _, _ => trivial
  | .force _, _ => trivial
  | .let_ (.force _) _, _ => trivial

theorem _root_.Flick.Tm.LzTop.rename {x : Pos} {e : Tm} (h : e.LzTop x) :
    ∀ ξ : Nat → Nat, (e.rename ξ).LzTop x := by
  induction h with
  | here hs => intro ξ; exact .here (hs.rename _)
  | lam _ ih => intro f; exact .lam (ih _)
  | fix _ ih => intro f; exact .fix (ih _)
  | task _ ih => intro f; exact .task (ih _)
  | await _ ih => intro f; exact .await (ih _)
  | lazy _ ih => intro f; exact .lazy (ih _)
  | force _ ih => intro f; exact .force (ih _)
  | raise _ ih => intro f; exact .raise (ih _)
  | scope _ ih => intro f; exact .scope (ih _)
  | proj _ ih => intro f; exact .proj (ih _)
  | drec _ ih => intro f; exact .drec (ih _)
  | appL _ ih => intro f; exact .appL (ih _)
  | appR _ ih => intro f; exact .appR (ih _)
  | letL _ ih => intro f; exact .letL (ih _)
  | letR _ ih => intro f; exact .letR (ih _)
  | letrecL _ ih => intro f; exact .letrecL (ih _)
  | letrecR _ ih => intro f; exact .letrecR (ih _)
  | rconsL _ ih => intro f; exact .rconsL (ih _)
  | rconsR _ ih => intro f; exact .rconsR (ih _)
  | updL _ ih => intro f; exact .updL (ih _)
  | updR _ ih => intro f; exact .updR (ih _)
  | matchL _ ih => intro f; exact .matchL (ih _)
  | matchR _ ih => intro f; exact .matchR (ih _)
  | acons _ ih => intro f; exact .acons (ih _)
  | aguardG _ ih => intro f; exact .aguardG (ih _)
  | aguardB _ ih => intro f; exact .aguardB (ih _)
  | aguardR _ ih => intro f; exact .aguardR (ih _)
  | tryL _ ih => intro f; exact .tryL (ih _)
  | tryR _ ih => intro f; exact .tryR (ih _)
  | handleV _ ih => intro f; exact .handleV (ih _)
  | handleA _ ih => intro f; exact .handleA (ih _)
  | hdlA _ ih => intro f; exact .hdlA (ih _)
  | hdlR _ ih => intro f; exact .hdlR (ih _)
  | hdlE _ ih => intro f; exact .hdlE (ih _)
  | inside _ ih => intro f; exact .inside (ih _)
  | hwith _ ih => intro f; exact .hwith (ih _)
  | hkeep _ ih => intro f; exact .hkeep (ih _)
  | hkarm _ ih => intro f; exact .hkarm (ih _)
  | hctl _ ih => intro f; exact .hctl (ih _)
  | hres _ ih => intro f; exact .hres (ih _)
  | hdone _ ih => intro f; exact .hdone (ih _)
  | unw _ ih => intro f; exact .unw (ih _)
  | settle _ ih => intro f; exact .settle (ih _)

theorem _root_.Flick.Tm.LzTop.subst {x : Pos} {e : Tm} (h : e.LzTop x) :
    ∀ σ : Nat → Tm, (e.subst σ).LzTop x := by
  induction h with
  | here hs => intro σ; exact .here (hs.subst _)
  | lam _ ih => intro f; exact .lam (ih _)
  | fix _ ih => intro f; exact .fix (ih _)
  | task _ ih => intro f; exact .task (ih _)
  | await _ ih => intro f; exact .await (ih _)
  | lazy _ ih => intro f; exact .lazy (ih _)
  | force _ ih => intro f; exact .force (ih _)
  | raise _ ih => intro f; exact .raise (ih _)
  | scope _ ih => intro f; exact .scope (ih _)
  | proj _ ih => intro f; exact .proj (ih _)
  | drec _ ih => intro f; exact .drec (ih _)
  | appL _ ih => intro f; exact .appL (ih _)
  | appR _ ih => intro f; exact .appR (ih _)
  | letL _ ih => intro f; exact .letL (ih _)
  | letR _ ih => intro f; exact .letR (ih _)
  | letrecL _ ih => intro f; exact .letrecL (ih _)
  | letrecR _ ih => intro f; exact .letrecR (ih _)
  | rconsL _ ih => intro f; exact .rconsL (ih _)
  | rconsR _ ih => intro f; exact .rconsR (ih _)
  | updL _ ih => intro f; exact .updL (ih _)
  | updR _ ih => intro f; exact .updR (ih _)
  | matchL _ ih => intro f; exact .matchL (ih _)
  | matchR _ ih => intro f; exact .matchR (ih _)
  | acons _ ih => intro f; exact .acons (ih _)
  | aguardG _ ih => intro f; exact .aguardG (ih _)
  | aguardB _ ih => intro f; exact .aguardB (ih _)
  | aguardR _ ih => intro f; exact .aguardR (ih _)
  | tryL _ ih => intro f; exact .tryL (ih _)
  | tryR _ ih => intro f; exact .tryR (ih _)
  | handleV _ ih => intro f; exact .handleV (ih _)
  | handleA _ ih => intro f; exact .handleA (ih _)
  | hdlA _ ih => intro f; exact .hdlA (ih _)
  | hdlR _ ih => intro f; exact .hdlR (ih _)
  | hdlE _ ih => intro f; exact .hdlE (ih _)
  | inside _ ih => intro f; exact .inside (ih _)
  | hwith _ ih => intro f; exact .hwith (ih _)
  | hkeep _ ih => intro f; exact .hkeep (ih _)
  | hkarm _ ih => intro f; exact .hkarm (ih _)
  | hctl _ ih => intro f; exact .hctl (ih _)
  | hres _ ih => intro f; exact .hres (ih _)
  | hdone _ ih => intro f; exact .hdone (ih _)
  | unw _ ih => intro f; exact .unw (ih _)
  | settle _ ih => intro f; exact .settle (ih _)

/-- A term of a frame is where it is evaluated. -/
theorem _root_.Flick.Tm.LzTop.frame {a : Tm} (h : a.LzTop .any) (F : Frame) :
    (F.plug a).LzTop .any := by
  cases F with
  | appL => exact .appL h
  | appR => exact .appR h
  | let_ => exact .letL h
  | match_ => exact .matchL h
  | rconsL => exact .rconsL h
  | rconsR => exact .rconsR h
  | proj => exact .proj h
  | updL => exact .updL h
  | updR => exact .updR h
  | await => exact .await h
  | try_ => exact .tryL h
  | raise => exact .raise h
  | inside => exact .inside h
  | hwith => exact .hwith h
  | hctlA => exact .hctl h
  | hresB => exact .hres h
  | hdone => exact .hdone h
  | hkeep => exact .hkeep h
  | hkarmA => exact .hkarm h
  | force => exact .force h
  | settle => exact .settle h

theorem _root_.Flick.Tm.LzTop.plug {a : Tm} (h : a.LzTop .any) :
    ∀ E : List Frame, (plug E a).LzTop .any
  | [] => h
  | F :: E => by
    simp only [Flick.plug]
    exact Tm.LzTop.plug (h.frame F) E

end Flick.Equiv
