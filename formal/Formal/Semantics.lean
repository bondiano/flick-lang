import Formal.Rules

/-!
# Dynamics of λFlick (core.md §3)

A configuration is `⟨μ, e⟩` with `μ` a list of values: location `ℓ` is the
index into `μ`, and `Ref.make` allocates `μ.length`. `Σ₀` is a parameter.
-/

namespace Flick

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
  | with_ (θ : Tm)
  | raise

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
  | .with_ θ, e => .with_ θ e
  | .raise, e => .raise e

/-- Left-to-right order: a frame to the right of a value needs the value. -/
def Frame.ok : Frame → Prop
  | .appR v | .rconsR _ v | .updR v _ | .joinR v | .with_ v => Value v
  | _ => True

def Frame.isTry : Frame → Prop
  | .try_ _ => True
  | _ => False

/-- The service a `with θ` frame provides. -/
def Frame.lookup : Frame → Name → Option Tm
  | .with_ θ, S => θ.envLookup S
  | _, _ => none

/-- `E[e]`; the list starts at the innermost frame. -/
def plug : List Frame → Tm → Tm
  | [], e => e
  | F :: E, e => plug E (F.plug e)

/-- `lookup(E, S)` of core.md §3.2 without the fallback to `Σ₀`. -/
def lookupE : List Frame → Name → Option Tm
  | [], _ => none
  | F :: E, S => match F.lookup S with
    | some v => some v
    | none => lookupE E S

/-- Pure steps `→ₚ` of core.md §3.3, with `bind` and `join` (Р101). -/
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
  | withVal : Value v → PHead (.with_ θ v) v
  | bind : Value v → PHead (.bind S v) (.econs S v .enil)
  | join : IsEnv θ₁ → IsEnv θ₂ → PHead (.join θ₁ θ₂) (θ₁.envAppend θ₂)

/-- Runtime steps of core.md §3.4–3.5 at the redex, except `ask_S` and the
propagation of `raise`, which need the context. -/
inductive RHead : List Tm → Tm → List Tm → Tm → Prop
  | refMake : Value v → RHead μ (.app (.const .refMake) v) (μ ++ [v]) (.loc μ.length)
  | refGet : μ[l]? = some v → RHead μ (.app (.const .refGet) (.loc l)) μ v
  | refSet : Value v → l < μ.length →
      RHead μ (.app (.app (.const .refSet) (.loc l)) v) (μ.set l v) (.lit .unit)
  | await : RHead μ (.await (.task e)) μ e
  | build : Value v → v.build = some b → RHead μ (.provide v e) μ (.provide b e)
  | provideEnv : IsEnv θ → RHead μ (.provide θ e) μ (.with_ θ e)
  | tryRaise : Value v → RHead μ (.try_ (.raise v) A) μ (.handle v A)
  | handleYes : Value v → matchP p v = some δ →
      RHead μ (.handle v (.acons p b r)) μ (b.subst (inst δ))
  | handleNo : Value v → matchP p v = none →
      RHead μ (.handle v (.acons p b r)) μ (.handle v r)
  | hguardYes : Value v → matchP p v = some δ →
      RHead μ (.handle v (.aguard p g b r)) μ
        (.match_ (g.subst (inst δ))
          (.acons (.lit (.bool true)) (b.subst (inst δ))
            (.acons (.lit (.bool false)) (.handle v r) .anil)))
  | hguardNo : Value v → matchP p v = none →
      RHead μ (.handle v (.aguard p g b r)) μ (.handle v r)
  | handleNil : Value v → RHead μ (.handle v .anil) μ (.raise v)

/-- Outcome of a step: a configuration or a defect. -/
inductive Out
  | cfg (μ : List Tm) (e : Tm)
  | die

/-- Step labels: pure, runtime, or an `ask_S` answered by `Σ₀` (T2(b)). -/
inductive Lbl
  | pure
  | rt
  | root (S : Name)

def OkE (E : List Frame) : Prop := ∀ F ∈ E, Frame.ok F

/-- `⟨μ, e⟩ → ⟨μ', e'⟩` and `⟨μ, e⟩ → die` (core.md §3.3–3.5). -/
inductive Step (root : Name → Option Tm) : Lbl → List Tm → Tm → Out → Prop
  | pure : OkE E → PHead e e' → Step root .pure μ (plug E e) (.cfg μ (plug E e'))
  | rt : OkE E → RHead μ e μ' e' → Step root .rt μ (plug E e) (.cfg μ' (plug E e'))
  | askWith : OkE E → lookupE E S = some v →
      Step root .rt μ (plug E (.app (.const (.ask S)) (.lit .unit))) (.cfg μ (plug E v))
  | askRoot : OkE E → lookupE E S = none → root S = some v →
      Step root (.root S) μ (plug E (.app (.const (.ask S)) (.lit .unit))) (.cfg μ (plug E v))
  | raise {F : Frame} : OkE E → F.ok → ¬ F.isTry → Value v →
      Step root .rt μ (plug E (F.plug (.raise v))) (.cfg μ (plug E (.raise v)))
  | todo : OkE E → Step root .pure μ (plug E .todo) .die

/-- `⟨μ, e⟩ →* ⟨μ', e'⟩` where every step has a label in `Q`. -/
inductive Steps (root : Name → Option Tm) (Q : Lbl → Prop) :
    List Tm → Tm → List Tm → Tm → Prop
  | refl : Steps root Q μ e μ e
  | step : Step root l μ e (.cfg μ₁ e₁) → Q l → Steps root Q μ₁ e₁ μ' e' → Steps root Q μ e μ' e'

end Flick
