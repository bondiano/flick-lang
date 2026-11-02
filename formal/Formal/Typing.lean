import Formal.Reach
import Formal.DictOps

/-!
# Values, patterns, shapes and subtyping (core.md §1.3, §2.2, §2.4)
-/

namespace Flick

/-- Values `v` of core.md §1.3 and §3.1. A partially applied constructor is a
spine with a `ctor` head (`docs/formal/core.md#a-partial-constructor-application-is-a-value`). A constant of arity 2 applied once is a value,
and so is a saturated `Layer.*` application. -/
inductive Value : Tm → Prop
  | lam : Value (.lam b)
  | fix : Value (.fix f)
  | lit : Value (.lit l)
  | loc : Value (.loc l)
  | const : Value (.const c)
  | ctor : Value (.ctor K)
  | rnil : Value .rnil
  | rcons : Value e → Value r → Value (.rcons l e r)
  | ctorApp : Value f → f.head = .ctor K → Value e → Value (.app f e)
  | constApp : (c.arity = 2 ∨ c.layer) → Value e → Value (.app (.const c) e)
  | constApp2 : c.arity = 2 → c.layer → Value e₁ → Value e₂ →
      Value (.app (.app (.const c) e₁) e₂)
  | task : Value (.task e)
  | enil : Value .enil
  | econs : Value v → Value θ → Value (.econs S v θ)
  | cap : Value (.cap p)
  | own : Value (.own p)
  | drec : Value (.drec c r)

/-- Environment values `env θ` of core.md §3.1. -/
inductive IsEnv : Tm → Prop
  | enil : IsEnv .enil
  | econs : Value v → IsEnv θ → IsEnv (.econs S v θ)

theorem IsEnv.value {θ : Tm} (h : IsEnv θ) : Value θ := by
  induction h with
  | enil => exact .enil
  | econs hv _ ih => exact .econs hv ih

/-- `match(p, v)` of core.md §3.3: the values bound by `p`, left to right. -/
def matchP : Pat → Tm → Option (List Tm)
  | .wild, _ => some []
  | .var, v => some [v]
  | .lit l, .lit l' => if l = l' then some [] else none
  | .ctor K, .ctor K' => if K = K' then some [] else none
  | .app p q, .app f e => do
    let a ← matchP p f
    let b ← matchP q e
    pure (a ++ b)
  | .rany, .rnil => some []
  | .rany, .rcons .. => some []
  | .rfield l p r, v => do
    let x ← v.field l
    let a ← matchP p x
    let b ← matchP r v
    pure (a ++ b)
  | .or p q, v => (matchP p v).or (matchP q v)
  | _, _ => none

/-- Subtyping `τ ≤ τ'` of core.md §2.2 with (≤-Cap) of regions.md §2.2: only
arrows and `Eff` have it, and the whole value is compared by `⌈·⌉`. -/
inductive Le : Ty → Ty → Prop
  | refl : Le t t
  | arrow : Le a' a → Stage.le φ φ' → Le t t' →
      Caps.Sub (Ty.arrow a φ t C).reach (Ty.arrow a' φ' t' C').reach →
      Le (.arrow a φ t C) (.arrow a' φ' t' C')
  | eff : Row.Sub r r' → Row.Sub e e' → Le t t' →
      Caps.Sub (Ty.eff r e t C).reach (Ty.eff r' e' t' C').reach →
      Le (.eff r e t C) (.eff r' e' t' C')

/-- Types whose values patterns cannot inspect: all but base types, records and
nominal data, read at the head of an application. -/
def Ty.Opaque (t : Ty) : Prop :=
  match t.hd with
  | .base _ | .adt _ | .rnil | .rcons .. => False
  | _ => True

/-- `α₀ … αₖ₋₁`. -/
def idTs (k : Nat) : List Ty := (List.range k).map Ty.tvar

variable (S : Sig)

/-- `⊢ p : τ ⇒ Δ` of core.md §2.4; the flag is `false` for an unsaturated
constructor spine. -/
inductive PatTy : Bool → Pat → Ty → List Ty → Prop
  | wild : PatTy true .wild t []
  | var : PatTy true .var t [t]
  | lit : PatTy true (.lit l) (.base l.base) []
  | head {ts us : List Ty} : S.ctor K = some (k, ts, T) → us.length = k →
      PatTy false (.ctor K) (Sig.ctorTy ts T us) []
  | app : PatTy false p (.arrow a .pure t Caps.none) Δ₁ → PatTy true q a Δ₂ →
      PatTy false (.app p q) t (Δ₁ ++ Δ₂)
  | adt : PatTy false p U Δ → U.hd = .adt T → PatTy true p U Δ
  | rany : PatTy true .rany R []
  | rfield : R.field l = some t → PatTy true p t Δ₁ → PatTy true r R Δ₂ →
      PatTy true (.rfield l p r) R (Δ₁ ++ Δ₂)
  | or : PatTy true p t Δ → PatTy true q t Δ → PatTy true (.or p q) t Δ

/-- Shapes of closed values of a type, as far as patterns see them. The fields of
a constructor have the types of its declaration: a field of a parameter type is
opaque. -/
inductive Shp : Bool → Ty → Tm → Prop
  | hidden : t.Opaque → Shp true t v
  | lit : Shp true (.base l.base) (.lit l)
  | rnil : Shp true .rnil .rnil
  | rcons : Shp true t e → Shp true R r → Shp true (.rcons l t R) (.rcons l e r)
  | head {ts : List Ty} : S.ctor K = some (k, ts, T) →
      Shp false (.arrows ts (.tcon T (idTs k))) (.ctor K)
  | app : Shp false (.arrow a .pure t Caps.none) f → Shp true a e → Shp false t (.app f e)
  | adt : Shp false U v → U.hd = .adt T → T'.hd = .adt T → Shp true T' v

/-- `Exh(τ, Π)` of core.md §2.4. -/
def Exh (t : Ty) (ps : List Pat) : Prop :=
  ∀ v, Shp S true t v → ∃ p ∈ ps, (matchP p v).isSome

/-- The coverage premise of (Match), or of (Match-Val) (`docs/formal/core.md#the-residual-match-after-a-false-guard-fails-exh`). -/
def Covers (t : Ty) (ps : List Pat) (e : Tm) : Prop :=
  Exh S t ps ∨ (Value e ∧ ∃ p ∈ ps, (matchP p e).isSome)

/-- Patterns of a `try` arm (core.md §2.3 (Try)): `x` or `_` at `Errors ε_b`,
or a constructor pattern `E p̄` at the error type `E`. -/
inductive HPat (εb : Row) : Pat → Ty → List Ty → Prop
  | wild : HPat εb .wild (.errors εb) []
  | var : HPat εb .var (.errors εb) [.errors εb]
  | ctor : PatTy S false p (.adt T) Δ → HPat εb p (.adt T) Δ

/-- The errors an arm without `when` removes: the set `H` of (Try). -/
def Catches (εb : Row) (p : Pat) (θ : Ty) : Elem → Prop
  | .nm n => (θ = .errors εb ∧ εb n) ∨ (θ = .adt n ∧ Exh S (.adt n) [p])
  | _ => False

/-- The error type of the raised value `v` is removed by the arms or let
through: the value form of `ε_b ⊆ H + ε'`. -/
def HCov (v : Tm) (H ε' : Row) : Prop :=
  ∀ K k ts T, v.head = .ctor K → S.ctor K = some (k, ts, T) → H T ∨ ε' T

end Flick
