import Formal.Subst

/-!
# Statics of λFlick (core.md §2 and §3.6), stage P1a
-/

namespace Flick

/-- Values `v` of core.md §1.3 and §3.1. A partially applied constructor is a
spine with a `ctor` head; a partially applied constant has arity 2 and one
argument. Over-applied spines are ill-typed. -/
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
  | constApp : c.arity = 2 → Value e → Value (.app (.const c) e)

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

variable (S : Sig)

/-- `⊢ p : τ ⇒ Δ` of core.md §2.4. The flag is `false` for a constructor
spine that is not yet applied to all its arguments. -/
inductive PatTy : Bool → Pat → Ty → List Ty → Prop
  | wild : PatTy true .wild t []
  | var : PatTy true .var t [t]
  | lit : PatTy true (.lit l) (.base l.base) []
  | head {ts : List Ty} : S.ctor K = some (ts, T) → PatTy false (.ctor K) (.arrows ts (.adt T)) []
  | app : PatTy false p (.arrow a t) Δ₁ → PatTy true q a Δ₂ →
      PatTy false (.app p q) t (Δ₁ ++ Δ₂)
  | adt : PatTy false p (.adt T) Δ → PatTy true p (.adt T) Δ
  | rany : PatTy true .rany R []
  | rfield : R.field l = some t → PatTy true p t Δ₁ → PatTy true r R Δ₂ →
      PatTy true (.rfield l p r) R (Δ₁ ++ Δ₂)
  | or : PatTy true p t Δ → PatTy true q t Δ → PatTy true (.or p q) t Δ

/-- Shapes of closed values of a type, as far as patterns can see them:
functions and references are opaque. The flag is as in `PatTy`. -/
inductive Shp : Bool → Ty → Tm → Prop
  | arrow : Shp true (.arrow a t) v
  | ref : Shp true (.ref t) v
  | lit : Shp true (.base l.base) (.lit l)
  | rnil : Shp true .rnil .rnil
  | rcons : Shp true t e → Shp true R r → Shp true (.rcons l t R) (.rcons l e r)
  | head {ts : List Ty} : S.ctor K = some (ts, T) → Shp false (.arrows ts (.adt T)) (.ctor K)
  | app : Shp false (.arrow a t) f → Shp true a e → Shp false t (.app f e)
  | adt : Shp false (.adt T) v → Shp true (.adt T) v

/-- `Exh(τ, Π)` of core.md §2.4: every closed value of `τ` matches some `p ∈ Π`. -/
def Exh (t : Ty) (ps : List Pat) : Prop :=
  ∀ v, Shp S true t v → ∃ p ∈ ps, (matchP p v).isSome

/-- The coverage premise of (Match), or of (Match-Val) when the scrutinee is a
value matched by an arm without `when` (Р29). -/
def Covers (t : Ty) (ps : List Pat) (e : Tm) : Prop :=
  Exh S t ps ∨ (Value e ∧ ∃ p ∈ ps, (matchP p e).isSome)

/-- Judgment forms: an expression of a type, or match arms from the
scrutinee's type to the result type. -/
inductive J
  | ex (t : Ty)
  | alts (t t' : Ty)

/-- `Γ;Ψ ⊢ e : τ` of core.md §2.3 and §3.6 without stages. `Ψ` types locations. -/
inductive Typ (Ψ : List Ty) : List Ty → Tm → J → Prop
  | var : Γ[i]? = some t → Typ Ψ Γ (.var i) (.ex t)
  | lam : Typ Ψ (a :: Γ) b (.ex t) → Typ Ψ Γ (.lam b) (.ex (.arrow a t))
  | app : Typ Ψ Γ f (.ex (.arrow a t)) → Typ Ψ Γ e (.ex a) → Typ Ψ Γ (.app f e) (.ex t)
  | let_ : Typ Ψ Γ e (.ex a) → Typ Ψ (a :: Γ) b (.ex t) → Typ Ψ Γ (.let_ e b) (.ex t)
  | letrec : Typ Ψ (a :: .arrow a r :: Γ) f (.ex r) → Typ Ψ (.arrow a r :: Γ) b (.ex t) →
      Typ Ψ Γ (.letrec f b) (.ex t)
  | fix : Typ Ψ (a :: .arrow a r :: Γ) f (.ex r) → Typ Ψ Γ (.fix f) (.ex (.arrow a r))
  | lit : Typ Ψ Γ (.lit l) (.ex (.base l.base))
  | loc : Ψ[l]? = some t → Typ Ψ Γ (.loc l) (.ex (.ref t))
  | const : Typ Ψ Γ (.const c) (.ex c.ty)
  | ctor {ts : List Ty} : S.ctor K = some (ts, T) → Typ Ψ Γ (.ctor K) (.ex (.arrows ts (.adt T)))
  | rnil : Typ Ψ Γ .rnil (.ex .rnil)
  | rcons : l ∉ R.labels → Typ Ψ Γ e (.ex t) → Typ Ψ Γ r (.ex R) →
      Typ Ψ Γ (.rcons l e r) (.ex (.rcons l t R))
  | proj : Typ Ψ Γ e (.ex R) → R.field l = some t → Typ Ψ Γ (.proj e l) (.ex t)
  | update : Typ Ψ Γ e (.ex R) → R.field l = some t → Typ Ψ Γ e' (.ex t) →
      Typ Ψ Γ (.update e l e') (.ex R)
  | match_ : Typ Ψ Γ e (.ex t) → Typ Ψ Γ A (.alts t t') → Covers S t A.unguarded e →
      Typ Ψ Γ (.match_ e A) (.ex t')
  | anil : Typ Ψ Γ .anil (.alts t t')
  | acons : PatTy S true p t Δ → Typ Ψ (Δ ++ Γ) b (.ex t') → Typ Ψ Γ r (.alts t t') →
      Typ Ψ Γ (.acons p b r) (.alts t t')
  | aguard : PatTy S true p t Δ → Typ Ψ (Δ ++ Γ) g (.ex (.base .bool)) →
      Typ Ψ (Δ ++ Γ) b (.ex t') → Typ Ψ Γ r (.alts t t') →
      Typ Ψ Γ (.aguard p g b r) (.alts t t')
  | todo : Typ Ψ Γ .todo (.ex t)

/-- `⊢ μ : Ψ` of core.md §3.6: every cell holds a closed value of its type. -/
def StoreTy (Ψ : List Ty) (μ : List Tm) : Prop :=
  μ.length = Ψ.length ∧
    ∀ (i : Nat) v t, μ[i]? = some v → Ψ[i]? = some t → Value v ∧ Typ S Ψ [] v (.ex t)

end Flick
