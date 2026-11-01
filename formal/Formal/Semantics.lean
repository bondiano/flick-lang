import Formal.Typing

/-!
# Dynamics of λFlick (core.md §3), stage P1a

A configuration is `⟨μ, e⟩` with `μ` a list of values: location `ℓ` is the
index into `μ`, and `Ref.make` allocates `μ.length`.
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

/-- Left-to-right order: a frame to the right of a value needs the value. -/
def Frame.ok : Frame → Prop
  | .appR v => Value v
  | .rconsR _ v => Value v
  | .updR v _ => Value v
  | _ => True

/-- `E[e]`; the list starts at the innermost frame. -/
def plug : List Frame → Tm → Tm
  | [], e => e
  | F :: E, e => plug E (F.plug e)

/-- Pure steps `→ₚ` (core.md §3.3) and the `Ref` steps of §3.4, at the redex. -/
inductive Head : List Tm → Tm → List Tm → Tm → Prop
  | beta : Value v → Head μ (.app (.lam b) v) μ (b.subst (inst [v]))
  | fix : Value v → Head μ (.app (.fix f) v) μ (f.subst (inst [v, .fix f]))
  | let_ : Value v → Head μ (.let_ v b) μ (b.subst (inst [v]))
  | letrec : Head μ (.letrec f b) μ (b.subst (inst [.fix f]))
  | proj : Value v → v.field l = some w → Head μ (.proj v l) μ w
  | update : Value v → Value w → v.setField l w = some v' → Head μ (.update v l w) μ v'
  | matchYes : Value v → matchP p v = some δ →
      Head μ (.match_ v (.acons p b r)) μ (b.subst (inst δ))
  | matchNo : Value v → matchP p v = none → Head μ (.match_ v (.acons p b r)) μ (.match_ v r)
  | guardYes : Value v → matchP p v = some δ →
      Head μ (.match_ v (.aguard p g b r)) μ
        (.match_ (g.subst (inst δ))
          (.acons (.lit (.bool true)) (b.subst (inst δ))
            (.acons (.lit (.bool false)) (.match_ v r) .anil)))
  | guardNo : Value v → matchP p v = none →
      Head μ (.match_ v (.aguard p g b r)) μ (.match_ v r)
  | refMake : Value v → Head μ (.app (.const (.refMake t)) v) (μ ++ [v]) (.loc μ.length)
  | refGet : μ[l]? = some v → Head μ (.app (.const (.refGet t)) (.loc l)) μ v
  | refSet : Value v → l < μ.length →
      Head μ (.app (.app (.const (.refSet t)) (.loc l)) v) (μ.set l v) (.lit .unit)

/-- Outcome of a step: a configuration or a defect. -/
inductive Out
  | cfg (μ : List Tm) (e : Tm)
  | die

/-- `⟨μ, e⟩ → ⟨μ', e'⟩` and `⟨μ, e⟩ → die` (core.md §3.4, §3.5). -/
inductive Step : List Tm → Tm → Out → Prop
  | head : (∀ F ∈ E, Frame.ok F) → Head μ e μ' e' → Step μ (plug E e) (.cfg μ' (plug E e'))
  | todo : (∀ F ∈ E, Frame.ok F) → Step μ (plug E .todo) .die

/-- `⟨μ, e⟩ →* ⟨μ', e'⟩`. -/
inductive Steps : List Tm → Tm → List Tm → Tm → Prop
  | refl : Steps μ e μ e
  | step : Step μ e (.cfg μ₁ e₁) → Steps μ₁ e₁ μ' e' → Steps μ e μ' e'

end Flick
