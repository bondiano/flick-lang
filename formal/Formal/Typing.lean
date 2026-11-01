import Formal.Subst

/-!
# Values, patterns, shapes and subtyping (core.md §1.3, §2.2, §2.4)
-/

namespace Flick

/-- Values `v` of core.md §1.3 and §3.1. A partially applied constructor is a
spine with a `ctor` head (Р100). A constant of arity 2 applied once is a value,
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

/-- Subtyping `τ ≤ τ'` of core.md §2.2: only arrows and `Eff` have it. -/
inductive Le : Ty → Ty → Prop
  | refl : Le t t
  | arrow : Le a' a → Stage.le φ φ' → Le t t' → Le (.arrow a φ t) (.arrow a' φ' t')
  | eff : Row.Sub r r' → Row.Sub e e' → Le t t' → Le (.eff r e t) (.eff r' e' t')

/-- Types whose values patterns cannot inspect. -/
def Ty.Opaque : Ty → Prop
  | .arrow .. | .ref _ | .eff .. | .layer .. | .errors _ | .env _ => True
  | _ => False

variable (S : Sig)

/-- `⊢ p : τ ⇒ Δ` of core.md §2.4; the flag is `false` for an unsaturated
constructor spine. -/
inductive PatTy : Bool → Pat → Ty → List Ty → Prop
  | wild : PatTy true .wild t []
  | var : PatTy true .var t [t]
  | lit : PatTy true (.lit l) (.base l.base) []
  | head {ts : List Ty} : S.ctor K = some (ts, T) → PatTy false (.ctor K) (.arrows ts (.adt T)) []
  | app : PatTy false p (.arrow a .pure t) Δ₁ → PatTy true q a Δ₂ →
      PatTy false (.app p q) t (Δ₁ ++ Δ₂)
  | adt : PatTy false p (.adt T) Δ → PatTy true p (.adt T) Δ
  | rany : PatTy true .rany R []
  | rfield : R.field l = some t → PatTy true p t Δ₁ → PatTy true r R Δ₂ →
      PatTy true (.rfield l p r) R (Δ₁ ++ Δ₂)
  | or : PatTy true p t Δ → PatTy true q t Δ → PatTy true (.or p q) t Δ

/-- Shapes of closed values of a type, as far as patterns see them. -/
inductive Shp : Bool → Ty → Tm → Prop
  | hidden : t.Opaque → Shp true t v
  | lit : Shp true (.base l.base) (.lit l)
  | rnil : Shp true .rnil .rnil
  | rcons : Shp true t e → Shp true R r → Shp true (.rcons l t R) (.rcons l e r)
  | head {ts : List Ty} : S.ctor K = some (ts, T) → Shp false (.arrows ts (.adt T)) (.ctor K)
  | app : Shp false (.arrow a .pure t) f → Shp true a e → Shp false t (.app f e)
  | adt : Shp false (.adt T) v → Shp true (.adt T) v

/-- `Exh(τ, Π)` of core.md §2.4. -/
def Exh (t : Ty) (ps : List Pat) : Prop :=
  ∀ v, Shp S true t v → ∃ p ∈ ps, (matchP p v).isSome

/-- The coverage premise of (Match), or of (Match-Val) (Р29). -/
def Covers (t : Ty) (ps : List Pat) (e : Tm) : Prop :=
  Exh S t ps ∨ (Value e ∧ ∃ p ∈ ps, (matchP p e).isSome)

/-- Patterns of a `try` arm (core.md §2.3 (Try)): `x` or `_` at `Errors ε_b`,
or a constructor pattern `E p̄` at the error type `E`. -/
inductive HPat (εb : Row) : Pat → Ty → List Ty → Prop
  | wild : HPat εb .wild (.errors εb) []
  | var : HPat εb .var (.errors εb) [.errors εb]
  | ctor : PatTy S false p (.adt T) Δ → HPat εb p (.adt T) Δ

/-- The errors an arm without `when` removes: the set `H` of (Try). -/
def Catches (εb : Row) (p : Pat) (θ : Ty) (n : Name) : Prop :=
  (θ = .errors εb ∧ εb n) ∨ (θ = .adt n ∧ Exh S (.adt n) [p])

/-- The error type of the raised value `v` is removed by the arms or let
through: the value form of `ε_b ⊆ H + ε'`. -/
def HCov (v : Tm) (H ε' : Row) : Prop :=
  ∀ K ts T, v.head = .ctor K → S.ctor K = some (ts, T) → H T ∨ ε' T

end Flick
