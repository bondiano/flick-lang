import Formal.Typing

/-!
# Typing rules of λFlick (core.md §2.3, §3.6; regions.md §2.3–§2.5, §3.4)

`Γ;Θ;Ψ ⊢ e : τ ! φ`. Every rule types its premises at the stage `φ` of the
conclusion, and subsumption (Sub) is folded into the rules whose result type
may be an arrow or an `Eff` (`Le … T`); `Typ.sub` proves (Sub) admissible.

`Γ` holds schemes; `Θ` the facts `Outlives C ϱ`. (Let-Gen) types its right-hand
side with `k` fresh variables, the indices `0 … k-1`, with `Γ`, `Θ` and `Ψ`
shifted past them, so that `ᾱ ∉ ftv(Γ) ∪ ftv(Ψ)` holds by construction; (Scope)
does the same with one variable, its region `s`, which is then not in `fr(Γ)`,
`fr(τ)`, `fr(r)` or `fr(ε)`.
-/

namespace Flick

/-- The row of errors of `raise e` with `e : E ῡ`: `{E}` and `⌈E ῡ⌉`. -/
def Row.err (E : Name) (U : Ty) : Row := Row.union (Row.single E) (Row.ofCaps U.reach)

/-- Judgment forms: an expression, `match` arms, `try` arms. For `try` arms,
`ε₀` is the error row of the body and `H` the errors the arms remove. -/
inductive J
  | ex (t : Ty) (φ : Stage)
  | alts (t t' : Ty) (φ : Stage)
  | halts (ε₀ H : Row) (t : Ty) (φ : Stage)

variable (S : Sig)

inductive Typ : List Ty → Facts → List Scheme → Tm → J → Prop
  | var {ts : List Ty} : Γ[i]? = some σ → ts.length = σ.k → σ.Holds Θ ts → Le (σ.inst ts) T →
      Typ Ψ Θ Γ (.var i) (.ex T φ)
  | lam : Typ Ψ Θ (.mono a :: Γ) b (.ex t ψ) → b.NoIns →
      Caps.Sub (held Ψ Γ (.lam b)) (Ty.arrow a ψ t C).reach → Le (.arrow a ψ t C) T →
      Typ Ψ Θ Γ (.lam b) (.ex T φ)
  | app : Typ Ψ Θ Γ f (.ex (.arrow a ψ t C) φ) → Typ Ψ Θ Γ e (.ex a φ) → ψ.le φ → Le t T →
      Typ Ψ Θ Γ (.app f e) (.ex T φ)
  | let_ : Typ Ψ Θ Γ e (.ex a φ) → Typ Ψ Θ (.mono a :: Γ) b (.ex t φ) →
      Typ Ψ Θ Γ (.let_ e b) (.ex t φ)
  | letGen : Typ (Ψ.map (Ty.up k)) (Facts.union (Θ.substT (shiftV k)) Φ) (Γ.map (Scheme.up k)) e
        (.ex a .pure) →
      Typ Ψ Θ (⟨k, Φ, a⟩ :: Γ) b (.ex t φ) → Typ Ψ Θ Γ (.let_ e b) (.ex t φ)
  | letrec : Typ Ψ Θ (.mono a :: .mono (.arrow a ψ r C) :: Γ) f (.ex r ψ) → f.NoIns →
      Caps.Sub (held Ψ Γ (.fix f)) (Ty.arrow a ψ r C).reach →
      Typ Ψ Θ (.mono (.arrow a ψ r C) :: Γ) b (.ex t φ) → Typ Ψ Θ Γ (.letrec f b) (.ex t φ)
  | fix : Typ Ψ Θ (.mono a :: .mono (.arrow a ψ r C) :: Γ) f (.ex r ψ) → f.NoIns →
      Caps.Sub (held Ψ Γ (.fix f)) (Ty.arrow a ψ r C).reach → Le (.arrow a ψ r C) T →
      Typ Ψ Θ Γ (.fix f) (.ex T φ)
  | lit : Typ Ψ Θ Γ (.lit l) (.ex (.base l.base) φ)
  | loc : Ψ[l]? = some t → Typ Ψ Θ Γ (.loc l) (.ex (.ref t) φ)
  | const : c.ok Θ ι → Le (c.ty S ι) T → Typ Ψ Θ Γ (.const c) (.ex T φ)
  | ctor {ts us : List Ty} : S.ctor K = some (k, ts, T₀) → us.length = k →
      Le (Sig.ctorTy ts T₀ us) T → Typ Ψ Θ Γ (.ctor K) (.ex T φ)
  | rnil : Typ Ψ Θ Γ .rnil (.ex .rnil φ)
  | rcons : R.IsRec → l ∉ R.labels → Typ Ψ Θ Γ e (.ex t φ) → Typ Ψ Θ Γ r (.ex R φ) →
      Typ Ψ Θ Γ (.rcons l e r) (.ex (.rcons l t R) φ)
  | proj : Typ Ψ Θ Γ e (.ex R φ) → R.field l = some t → Le t T → Typ Ψ Θ Γ (.proj e l) (.ex T φ)
  | update : Typ Ψ Θ Γ e (.ex R φ) → R.field l = some t → Typ Ψ Θ Γ e' (.ex t φ) →
      Typ Ψ Θ Γ (.update e l e') (.ex R φ)
  | match_ : Typ Ψ Θ Γ e (.ex t φ) → Typ Ψ Θ Γ A (.alts t t' φ) → Covers S t A.unguarded e →
      Typ Ψ Θ Γ (.match_ e A) (.ex t' φ)
  | anil : Typ Ψ Θ Γ .anil (.alts t t' φ)
  | acons : PatTy S true p t Δ → Typ Ψ Θ (Δ.map .mono ++ Γ) b (.ex t' φ) →
      Typ Ψ Θ Γ r (.alts t t' φ) → Typ Ψ Θ Γ (.acons p b r) (.alts t t' φ)
  | aguard : PatTy S true p t Δ → Typ Ψ Θ (Δ.map .mono ++ Γ) g (.ex (.base .bool) φ) →
      Typ Ψ Θ (Δ.map .mono ++ Γ) b (.ex t' φ) → Typ Ψ Θ Γ r (.alts t t' φ) →
      Typ Ψ Θ Γ (.aguard p g b r) (.alts t t' φ)
  | todo : Typ Ψ Θ Γ .todo (.ex t φ)
  | task : Typ Ψ Θ Γ e (.ex t ψ) → ψ.le (.rt r ε) → e.NoIns →
      Caps.Sub (held Ψ Γ (.task e)) (Ty.eff r ε t C).reach → Le (.eff r ε t C) T →
      Typ Ψ Θ Γ (.task e) (.ex T φ)
  | await : Typ Ψ Θ Γ e (.ex (.eff r ε t C) φ) → (Stage.rt r ε).le φ → Le t T →
      Typ Ψ Θ Γ (.await e) (.ex T φ)
  | raise : Typ Ψ Θ Γ e (.ex (.errors ε) φ) → (Stage.rt Row.none ε).le φ →
      Typ Ψ Θ Γ (.raise e) (.ex t φ)
  | raiseAdt : Typ Ψ Θ Γ e (.ex U φ) → U.hd = .adt E → (Stage.rt Row.none (Row.err E U)).le φ →
      Typ Ψ Θ Γ (.raise e) (.ex t φ)
  | errVal : Value e → Typ Ψ Θ Γ e (.ex U φ) → U.hd = .adt E → Row.Sub (Row.err E U) ε →
      Typ Ψ Θ Γ e (.ex (.errors ε) φ)
  | tryR : Typ Ψ Θ Γ e (.ex t (.rt r ε₀)) → Typ Ψ Θ Γ A (.halts ε₀ H t φ) →
      (Stage.rt r ε').le φ → Row.Sub ε₀ (Row.union H ε') → Typ Ψ Θ Γ (.try_ e A) (.ex t φ)
  | tryP : Typ Ψ Θ Γ e (.ex t .pure) → Typ Ψ Θ Γ A (.halts Row.none H t φ) →
      Typ Ψ Θ Γ (.try_ e A) (.ex t φ)
  | handle : Value v → Typ Ψ Θ Γ v (.ex (.errors ε₀) .pure) → Typ Ψ Θ Γ A (.halts ε₀ H t φ) →
      HCov S v H ε' → (∀ a, ε₀ (.atm a) → ε' (.atm a)) → (Stage.rt Row.none ε').le φ →
      Typ Ψ Θ Γ (.handle v A) (.ex t φ)
  | hnil : (∀ n, ¬ H n) → Typ Ψ Θ Γ .anil (.halts ε₀ H t φ)
  | hcons : HPat S ε₀ p θ Δ → Typ Ψ Θ (Δ.map .mono ++ Γ) b (.ex t φ) →
      Typ Ψ Θ Γ r (.halts ε₀ H' t φ) → (∀ n, H n → Catches S ε₀ p θ n ∨ H' n) →
      Typ Ψ Θ Γ (.acons p b r) (.halts ε₀ H t φ)
  | hguard : HPat S ε₀ p θ Δ → Typ Ψ Θ (Δ.map .mono ++ Γ) g (.ex (.base .bool) φ) →
      Typ Ψ Θ (Δ.map .mono ++ Γ) b (.ex t φ) → Typ Ψ Θ Γ r (.halts ε₀ H' t φ) → Row.Sub H H' →
      Typ Ψ Θ Γ (.aguard p g b r) (.halts ε₀ H t φ)
  | provideL : Typ Ψ Θ Γ l (.ex (.layer i ε₂ o) φ) → Typ Ψ Θ Γ e (.ex t φ₀) →
      φ₀.le (.rt (Row.union o r) ε) → (Stage.rt (Row.union i r) (Row.union ε₂ ε)).le φ →
      Typ Ψ Θ Γ (.provide l e) (.ex t φ)
  | provideE : Typ Ψ Θ Γ l (.ex (.env o) φ) → Typ Ψ Θ Γ e (.ex t φ₀) →
      φ₀.le (.rt (Row.union o r) ε) → (Stage.rt r ε).le φ → Typ Ψ Θ Γ (.provide l e) (.ex t φ)
  | with_ : Value θ → Typ Ψ Θ Γ θ (.ex (.env ρ) .pure) → Typ Ψ Θ Γ e (.ex t φ₀) →
      φ₀.le (.rt (Row.union ρ r) ε) → (Stage.rt r ε).le φ → Typ Ψ Θ Γ (.with_ θ e) (.ex t φ)
  | enil : (∀ n, ¬ ρ n) → Typ Ψ Θ Γ .enil (.ex (.env ρ) φ)
  | econs : Value v → IsEnv θ → Typ Ψ Θ Γ v (.ex (S.impl N) φ) → Typ Ψ Θ Γ θ (.ex (.env ρ) φ) →
      (∀ n, ρ' n → n = .nm N ∨ ρ n) → Typ Ψ Θ Γ (.econs N v θ) (.ex (.env ρ') φ)
  | bind : Typ Ψ Θ Γ e (.ex (S.impl N) φ) → (∀ n, ρ n → n = .nm N) →
      Typ Ψ Θ Γ (.bind N e) (.ex (.env ρ) φ)
  | join : Typ Ψ Θ Γ e₁ (.ex (.env ρ₁) φ) → Typ Ψ Θ Γ e₂ (.ex (.env ρ₂) φ) →
      (∀ n, ρ n → ρ₁ n ∨ ρ₂ n) → Typ Ψ Θ Γ (.join e₁ e₂) (.ex (.env ρ) φ)
  | scope : e.NoIns → (∀ C s, F C s → s = .tvar 0 ∧ ¬ C (.v 0)) →
      Typ (Ψ.map (Ty.up 1)) (Facts.union (Θ.substT (shiftV 1)) F)
        (.mono (.owner (.tvar 0)) :: Γ.map (Scheme.up 1)) e (.ex (t.up 1) φ') →
      φ'.le (.rt (Row.union (Row.scope (.tvar 0)) (r.substT (shiftV 1))) (ε.substT (shiftV 1))) →
      (Stage.rt r ε).le φ → Le t T → Typ Ψ Θ Γ (.scope e) (.ex T φ)
  | inside : Typ Ψ Θ Γ e (.ex t φ') → φ'.le (.rt (Row.union (Row.scope (.rg p)) r) ε) →
      (Stage.rt r ε).le φ → Le t T → Typ Ψ Θ Γ (.inside p e) (.ex T φ)
  | cap : Typ Ψ Θ Γ (.cap p) (.ex (.hnd (.rg p)) φ)
  | own : Typ Ψ Θ Γ (.own p) (.ex (.owner (.rg p)) φ)

/-- The facts of a running configuration for T1: any. The facts a scope gives its
region are about everything outside it; T4 needs them to hold of the order in
which regions are created. -/
def Facts.all : Facts := fun _ _ => True

/-- `⊢ μ : Ψ` of core.md §3.6. -/
def StoreTy (Ψ : List Ty) (μ : List Tm) : Prop :=
  μ.length = Ψ.length ∧
    ∀ (i : Nat) v t, μ[i]? = some v → Ψ[i]? = some t →
      Value v ∧ Typ S Ψ Facts.all [] v (.ex t .pure)

/-- The root environment `Σ₀` of core.md §2.6: closed values of `S.Impl`. -/
def RootTy (root : Name → Option Tm) : Prop :=
  ∀ N v, root N = some v → Value v ∧ Typ S [] Facts.none [] v (.ex (S.impl N) .pure)

end Flick
