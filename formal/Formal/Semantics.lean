import Formal.Rules
import Formal.EvalCtx

/-!
# Dynamics of λFlick (core.md §3, regions.md §3)

A thread steps in `⟨Λ, μ, e⟩`: `μ` is a list of values, location `ℓ` is the
index into `μ`, and `Ref.make` allocates `μ.length`; `Λ` is a list of run-time
regions, region `π` is the index into `Λ`, and `scope` creates `Λ.length`.
`Σ₀` is a parameter. A configuration is a main thread with `Λ` and `μ`; the
children of a live region step between its steps (`CStep`).
-/

namespace Flick

/-- A run-time region (regions.md §3.1): live or closed, its children `K(π)` and
its finalizers `F(π)`, both computations to run. -/
structure Rgn where
  live : Bool
  kids : List Tm
  fins : List Tm

def Rgn.new : Rgn := ⟨true, [], []⟩

/-- A live region. -/
def Live (Λ : List Rgn) (p : Nat) : Prop := ∃ g, Λ[p]? = some g ∧ g.live = true

/-- Pure steps `→ₚ` of core.md §3.3, with `bind` and `join` (`docs/formal/core.md#bind_s-and-join-steps-are-pure`). -/
inductive PHead : Tm → Tm → Prop
  | beta : Value v → PHead (.app (.lam b) v) (b.subst (inst [v]))
  | fix : Value v → PHead (.app (.fix f) v) (f.subst (inst [v, .fix f]))
  | let_ : Value v → PHead (.let_ v b) (b.subst (inst [v]))
  | letrec : PHead (.letrec f b) (b.subst (inst [.fix f]))
  | proj : Value v → v.field l = some w → PHead (.proj v l) w
  | update : Value v → Value w → v.setField l w = some v' → PHead (.update v l w) v'
  | matchYes : Value v → matchP p v = some δ → PHead (.match_ v (.acons p b r)) (b.subst (inst δ))
  | matchNo : Value v → matchP p v = none → PHead (.match_ v (.acons p b r)) (.match_ v r)
  | guardYes : Value v → matchP p v = some δ →
      PHead (.match_ v (.aguard p g b r))
        (.match_ (g.subst (inst δ))
          (.acons (.lit (.bool true)) (b.subst (inst δ))
            (.acons (.lit (.bool false)) (.match_ v r) .anil)))
  | guardNo : Value v → matchP p v = none → PHead (.match_ v (.aguard p g b r)) (.match_ v r)
  | tryVal : Value v → PHead (.try_ v A) v
  | withVal : Value v → PHead (.with_ q θ v) v
  | bind : Value v → PHead (.bind S v) (.econs S v .enil)
  | join : IsEnv θ₁ → IsEnv θ₂ → PHead (.join θ₁ θ₂) (θ₁.envAppend θ₂)
  | intEq : PHead (.app (.app (.const .intEq) (.lit (.int a))) (.lit (.int b))) (.lit (.bool (a == b)))
  -- the value of the body of a handler goes to its `return` function
  | hwithVal {al : Tm} : Value v → PHead (.hwith p ℓ E al r v) (.app r v)
  -- the value of a `tail` arm resumes the body under the handler
  | hctlTail {al : Tm} {Ei : List Frame} : OkE Ei → k = plug Ei (.hole p) → Value w →
      PHead (.hctl p ℓ κ true E al r w k) (.hwith p ℓ E al r (plug Ei w))
  -- a `control` arm that ends without resuming lets the body go: it unwinds to the
  -- handler, closing its regions, and the arm's value is the handler's
  | hctlDisc {al : Tm} {Ei : List Frame} : OkE Ei → k = plug Ei (.hole p) → Value w →
      PHead (.hctl p ℓ κ false E al r w k) (.hwith p ℓ E al r (plug Ei (.unw p ℓ w)))
  -- the value of a resumed body is what `resume` gives the arm
  | hresVal {A : List Frame} : OkE A → a = plug A (.hole p) → Value w →
      PHead (.hres p a w) (.hdone p (plug A w))
  | hdoneVal : Value v → PHead (.hdone p v) v

/-- `Λ[π] := g`. -/
def setRgn (Λ : List Rgn) (p : Nat) (g : Rgn) : List Rgn := Λ.set p g

/-- Runtime steps of core.md §3.4–3.5 and regions.md §3.2–§3.3 at the redex,
except `ask_S` and the propagation of `raise`, which need the context. An
acquisition registers the use that releases it as a finalizer. A finalizer runs
as a child once the body is finished and the earlier children are, the last one
first; the region closes when none is left. -/
inductive RHead : List Rgn → List Tm → Tm → List Rgn → List Tm → Tm → Prop
  | refMake : Value v → RHead Λ μ (.app (.const .refMake) v) Λ (μ ++ [v]) (.loc μ.length)
  | refGet : μ[l]? = some v → RHead Λ μ (.app (.const .refGet) (.loc l)) Λ μ v
  | refSet : Value v → l < μ.length →
      RHead Λ μ (.app (.app (.const .refSet) (.loc l)) v) Λ (μ.set l v) (.lit .unit)
  | await : RHead Λ μ (.await (.task e)) Λ μ e
  | build : Value v → v.build = some b → RHead Λ μ (.provide v e) Λ μ (.provide b e)
  | provideEnv : IsEnv θ → RHead Λ μ (.provide θ e) (Λ ++ [Rgn.new]) μ
      (.inside Λ.length (.with_ Λ.length θ e))
  | tryRaise : Value v → RHead Λ μ (.try_ (.raise v) A) Λ μ (.handle v A)
  | handleYes : Value v → matchP p v = some δ →
      RHead Λ μ (.handle v (.acons p b r)) Λ μ (b.subst (inst δ))
  | handleNo : Value v → matchP p v = none →
      RHead Λ μ (.handle v (.acons p b r)) Λ μ (.handle v r)
  | hguardYes : Value v → matchP p v = some δ →
      RHead Λ μ (.handle v (.aguard p g b r)) Λ μ
        (.match_ (g.subst (inst δ))
          (.acons (.lit (.bool true)) (b.subst (inst δ))
            (.acons (.lit (.bool false)) (.handle v r) .anil)))
  | hguardNo : Value v → matchP p v = none →
      RHead Λ μ (.handle v (.aguard p g b r)) Λ μ (.handle v r)
  | handleNil : Value v → RHead Λ μ (.handle v .anil) Λ μ (.raise v)
  | scope : RHead Λ μ (.scope e) (Λ ++ [Rgn.new]) μ
      (.inside Λ.length (e.subst (inst [.own Λ.length])))
  | open_ : Λ[p]? = some g → g.live = true →
      RHead Λ μ (.app (.const .open_) (.own p))
        (setRgn Λ p ⟨g.live, g.kids, g.fins ++ [.app (.const .op) (.cap p)]⟩) μ (.cap p)
  | op : Λ[p]? = some g → g.live = true →
      RHead Λ μ (.app (.const .op) (.cap p)) Λ μ (.lit .unit)
  | finRun {fs : List Tm} : Λ[p]? = some g → g.live = true → Final w → (∀ k ∈ g.kids, Final k) →
      g.fins = fs ++ [f] →
      RHead Λ μ (.inside p w) (setRgn Λ p ⟨g.live, g.kids ++ [f], fs⟩) μ (.inside p w)
  | close : Λ[p]? = some g → g.live = true → Final w → (∀ k ∈ g.kids, Final k) → g.fins = [] →
      RHead Λ μ (.inside p w) (setRgn Λ p ⟨false, g.kids, []⟩) μ w
  -- an arm that raises or unwinds lets the body go, and goes on raising or
  -- unwinding from the handler; a resumed body that does gives it to the arm
  | hctlRaise {al : Tm} {tl : Bool} {Ei : List Frame} : OkE Ei → k = plug Ei (.hole p) → Value w →
      RHead Λ μ (.hctl p ℓ κ tl E al r (.raise w) k) Λ μ (.hwith p ℓ E al r (plug Ei (.unw p ℓ (.raise w))))
  | hctlUnw {al : Tm} {tl : Bool} {Ei : List Frame} : OkE Ei → k = plug Ei (.hole p) →
      RHead Λ μ (.hctl p ℓ κ tl E al r (.unw q ℓ' a) k) Λ μ
        (.hwith p ℓ E al r (plug Ei (.unw p ℓ (.unw q ℓ' a))))
  | hresRaise {A : List Frame} : OkE A → a = plug A (.hole p) → Value w →
      RHead Λ μ (.hres p a (.raise w)) Λ μ (.hdone p (plug A (.raise w)))
  | hresUnw {A : List Frame} : OkE A → a = plug A (.hole p) →
      RHead Λ μ (.hres p a (.unw q ℓ' b)) Λ μ (.hdone p (plug A (.unw q ℓ' b)))
  -- the body has unwound to its handler, which runs what the signal holds
  | hwithCatch {al : Tm} : RHead Λ μ (.hwith p ℓ E al r (.unw p ℓ a)) Λ μ a
  -- a handler is the region `|Λ|` around its body, which `resume` holds, and the cell
  -- `|μ|`, which holds the type of its value
  | hdl {al : Tm} : RHead Λ μ (.hdl E al r e) (Λ ++ [Rgn.new]) (μ ++ [.lam .todo])
      (.inside Λ.length (.hwith Λ.length μ.length E al r e))

/-- Outcome of a step: a configuration or a defect. -/
inductive Out
  | cfg (Λ : List Rgn) (μ : List Tm) (e : Tm)
  | die

/-- Step labels: pure, runtime, or an `ask_S` answered by `Σ₀` (T2(b)). -/
inductive Lbl
  | pure
  | rt
  | root (S : Name)


/-- `⟨Λ, μ, e⟩ → ⟨Λ', μ', e'⟩` and `⟨Λ, μ, e⟩ → die` of one thread (core.md
§3.3–3.5, regions.md §3.2–§3.3). A global steps to its body, a pure step. -/
inductive Step (root : Roots) : Lbl → List Rgn → List Tm → Tm → Out → Prop
  | pure : OkE E → PHead e e' → Step root .pure Λ μ (plug E e) (.cfg Λ μ (plug E e'))
  | rt : OkE E → RHead Λ μ e Λ' μ' e' → Step root .rt Λ μ (plug E e) (.cfg Λ' μ' (plug E e'))
  | askWith : OkE E → lookupE E S = some (q, v) → Live Λ q →
      Step root .rt Λ μ (plug E (.app (.const (.ask S)) (.lit .unit))) (.cfg Λ μ (plug E v))
  | addFin : OkE E → Λ[p]? = some g → g.live = true → Value t →
      Step root .rt Λ μ (plug E (.app (.app (.const .addFin) (.own p)) t))
        (.cfg (setRgn Λ p ⟨g.live, g.kids, g.fins ++ [withs E (.await t)]⟩) μ (plug E (.lit .unit)))
  | spawn : OkE E → Λ[p]? = some g → g.live = true → Value t →
      Step root .rt Λ μ (plug E (.app (.app (.const .spawn) (.own p)) t))
        (.cfg (setRgn Λ p ⟨g.live, g.kids ++ [withs E (.await t)], g.fins⟩) μ (plug E (.lit .unit)))
  | askRoot : OkE E → lookupE E S = none → root.svc S = some v →
      Step root (.root S) Λ μ (plug E (.app (.const (.ask S)) (.lit .unit)))
        (.cfg Λ μ (plug E v))
  | raise {F : Frame} : OkE E → F.ok → ¬ F.isTry → ¬ F.isInside → ¬ F.isCtl → Value v →
      Step root .rt Λ μ (plug E (F.plug (.raise v))) (.cfg Λ μ (plug E (.raise v)))
  -- unwinding to the handler `p` passes `try`, as no error does
  | unw {F : Frame} : OkE E → F.ok → ¬ F.isInside → ¬ F.isCtl → ¬ F.catches p ℓ →
      Step root .rt Λ μ (plug E (F.plug (.unw p ℓ a))) (.cfg Λ μ (plug E (.unw p ℓ a)))
  -- an operation runs the arm of the nearest handler with one for it: `abort` after
  -- the body unwinds to the handler, `tail` and `control` while the body waits there
  | performAbort {Ei : List Frame} {Ef : Name} {al : Tm} : OkE E → OkE Ei → (∀ G ∈ Ei, G.handles Ef o = none) → al.armOf o = some b →
      Value v →
      Step root .rt Λ μ (plug E (.hwith p ℓ Ef al r (plug Ei (.app (.const (.perform Ef o .abort)) v))))
        (.cfg Λ μ (plug E (.hwith p ℓ Ef al r (plug Ei (.unw p ℓ (b.subst (inst [v])))))))
  | performTail {Ei : List Frame} {Ef : Name} {al : Tm} : OkE E → OkE Ei → (∀ G ∈ Ei, G.handles Ef o = none) → al.armOf o = some b →
      Value v →
      Step root .rt Λ μ (plug E (.hwith p ℓ Ef al r (plug Ei (.app (.const (.perform Ef o .tail)) v))))
        (.cfg Λ μ (plug E (.hctl p ℓ 0 true Ef al r (b.subst (inst [v])) (plug Ei (.hole p)))))
  -- `resume` of a `control` arm is `kont p κ`: its type is that of the new cell `κ`
  | performCtl {Ei : List Frame} {Ef : Name} {al : Tm} : OkE E → OkE Ei → (∀ G ∈ Ei, G.handles Ef o = none) → al.armOf o = some b →
      Value v →
      Step root .rt Λ μ (plug E (.hwith p ℓ Ef al r (plug Ei (.app (.const (.perform Ef o .control)) v))))
        (.cfg Λ (μ ++ [.lam .todo]) (plug E (.hctl p ℓ μ.length false Ef al r
          (b.subst (inst [v, .kont p μ.length])) (plug Ei (.hole p)))))
  -- `resume` runs the body under the handler, the arm waiting for how it ends
  | resume {Ef : Name} {al : Tm} {Ei : List Frame} {ℓh : Nat} : OkE E → OkE A → OkE Ei → k = plug Ei (.hole p) →
      (∀ G ∈ A, ¬ G.resumes p) → Value u →
      Step root .rt Λ μ (plug E (.hctl p ℓh ℓ false Ef al r (plug A (.app (.kont p ℓ) u)) k))
        (.cfg Λ μ (plug E (.hres p (plug A (.hole p)) (.hwith p ℓh Ef al r (plug Ei u)))))
  -- the body resumes once, in the thread of its handler, while its arm runs
  | resumeDie : OkE E → Value u → ¬ Resumable E p ℓ →
      Step root .rt Λ μ (plug E (.app (.kont p ℓ) u)) .die
  | todo : OkE E → Step root .pure Λ μ (plug E .todo) .die
  | glob : OkE E → root.glob g = some v → Step root .pure Λ μ (plug E (.glob g)) (.cfg Λ μ (plug E v))

/-- `⟨Λ, μ, e⟩ →* ⟨Λ', μ', e'⟩` of one thread where every step has a label in `Q`. -/
inductive Steps (root : Roots) (Q : Lbl → Prop) :
    List Rgn → List Tm → Tm → List Rgn → List Tm → Tm → Prop
  | refl : Steps root Q Λ μ e Λ μ e
  | step : Step root l Λ μ e (.cfg Λ₁ μ₁ e₁) → Q l → Steps root Q Λ₁ μ₁ e₁ Λ' μ' e' →
      Steps root Q Λ μ e Λ' μ' e'

/-- A configuration: the regions, the store and the main thread. -/
structure Cfg where
  rgs : List Rgn
  mem : List Tm
  main : Tm

/-- Replace the child `j` of region `p`. -/
def setKid (Λ : List Rgn) (p j : Nat) (k : Tm) : List Rgn :=
  match Λ[p]? with
  | some g => setRgn Λ p ⟨g.live, g.kids.set j k, g.fins⟩
  | none => Λ

/-- A step of the main thread, or of a child of a live region: children step
between any two steps of the main thread (regions.md §3.2). -/
inductive CStep (root : Roots) : Lbl → Cfg → Cfg → Prop
  | main : Step root l Λ μ e (.cfg Λ' μ' e') → CStep root l ⟨Λ, μ, e⟩ ⟨Λ', μ', e'⟩
  | kid : Λ[p]? = some g → g.live = true → g.kids[j]? = some k →
      Step root l Λ μ k (.cfg Λ' μ' k') → CStep root l ⟨Λ, μ, e⟩ ⟨setKid Λ' p j k', μ', e⟩

inductive CSteps (root : Roots) : Cfg → Cfg → Prop
  | refl : CSteps root c c
  | step : CStep root l c c₁ → CSteps root c₁ c' → CSteps root c c'

end Flick
