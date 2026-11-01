import Formal.Typing

/-!
# Typing rules of λFlick (core.md §2.3 and §3.6)

`Γ;Ψ ⊢ e : τ ! φ`. Every rule types its premises at the stage `φ` of the
conclusion, and subsumption (Sub) is folded into the rules whose result type
may be an arrow or an `Eff` (`Le … T`); `Typ.sub` proves (Sub) admissible.

`Γ` holds schemes. (Let-Gen) types its right-hand side with `k` fresh type
variables, the indices `0 … k-1`, with `Γ` and `Ψ` shifted past them, so that
`ᾱ ∉ ftv(Γ) ∪ ftv(Ψ)` holds by construction.
-/

namespace Flick

/-- Judgment forms: an expression, `match` arms, `try` arms. For `try` arms,
`ε₀` is the error row of the body and `H` the errors the arms remove. -/
inductive J
  | ex (t : Ty) (φ : Stage)
  | alts (t t' : Ty) (φ : Stage)
  | halts (ε₀ H : Row) (t : Ty) (φ : Stage)

variable (S : Sig)

inductive Typ : List Ty → List Scheme → Tm → J → Prop
  | var {ts : List Ty} : Γ[i]? = some σ → ts.length = σ.k → Le (σ.inst ts) T →
      Typ Ψ Γ (.var i) (.ex T φ)
  | lam : Typ Ψ (.mono a :: Γ) b (.ex t ψ) → Le (.arrow a ψ t) T → Typ Ψ Γ (.lam b) (.ex T φ)
  | app : Typ Ψ Γ f (.ex (.arrow a ψ t) φ) → Typ Ψ Γ e (.ex a φ) → ψ.le φ → Le t T →
      Typ Ψ Γ (.app f e) (.ex T φ)
  | let_ : Typ Ψ Γ e (.ex a φ) → Typ Ψ (.mono a :: Γ) b (.ex t φ) →
      Typ Ψ Γ (.let_ e b) (.ex t φ)
  | letGen : Typ (Ψ.map (Ty.up k)) (Γ.map (Scheme.up k)) e (.ex a .pure) →
      Typ Ψ (⟨k, a⟩ :: Γ) b (.ex t φ) → Typ Ψ Γ (.let_ e b) (.ex t φ)
  | letrec : Typ Ψ (.mono a :: .mono (.arrow a ψ r) :: Γ) f (.ex r ψ) →
      Typ Ψ (.mono (.arrow a ψ r) :: Γ) b (.ex t φ) → Typ Ψ Γ (.letrec f b) (.ex t φ)
  | fix : Typ Ψ (.mono a :: .mono (.arrow a ψ r) :: Γ) f (.ex r ψ) → Le (.arrow a ψ r) T →
      Typ Ψ Γ (.fix f) (.ex T φ)
  | lit : Typ Ψ Γ (.lit l) (.ex (.base l.base) φ)
  | loc : Ψ[l]? = some t → Typ Ψ Γ (.loc l) (.ex (.ref t) φ)
  | const : c.wf → Le (c.ty S τ) T → Typ Ψ Γ (.const c) (.ex T φ)
  | ctor {ts : List Ty} : S.ctor K = some (ts, T₀) → Le (.arrows ts (.adt T₀)) T →
      Typ Ψ Γ (.ctor K) (.ex T φ)
  | rnil : Typ Ψ Γ .rnil (.ex .rnil φ)
  | rcons : R.IsRec → l ∉ R.labels → Typ Ψ Γ e (.ex t φ) → Typ Ψ Γ r (.ex R φ) →
      Typ Ψ Γ (.rcons l e r) (.ex (.rcons l t R) φ)
  | proj : Typ Ψ Γ e (.ex R φ) → R.field l = some t → Le t T → Typ Ψ Γ (.proj e l) (.ex T φ)
  | update : Typ Ψ Γ e (.ex R φ) → R.field l = some t → Typ Ψ Γ e' (.ex t φ) →
      Typ Ψ Γ (.update e l e') (.ex R φ)
  | match_ : Typ Ψ Γ e (.ex t φ) → Typ Ψ Γ A (.alts t t' φ) → Covers S t A.unguarded e →
      Typ Ψ Γ (.match_ e A) (.ex t' φ)
  | anil : Typ Ψ Γ .anil (.alts t t' φ)
  | acons : PatTy S true p t Δ → Typ Ψ (Δ.map .mono ++ Γ) b (.ex t' φ) → Typ Ψ Γ r (.alts t t' φ) →
      Typ Ψ Γ (.acons p b r) (.alts t t' φ)
  | aguard : PatTy S true p t Δ → Typ Ψ (Δ.map .mono ++ Γ) g (.ex (.base .bool) φ) →
      Typ Ψ (Δ.map .mono ++ Γ) b (.ex t' φ) → Typ Ψ Γ r (.alts t t' φ) →
      Typ Ψ Γ (.aguard p g b r) (.alts t t' φ)
  | todo : Typ Ψ Γ .todo (.ex t φ)
  | task : Typ Ψ Γ e (.ex t ψ) → ψ.le (.rt r ε) → Le (.eff r ε t) T →
      Typ Ψ Γ (.task e) (.ex T φ)
  | await : Typ Ψ Γ e (.ex (.eff r ε t) φ) → (Stage.rt r ε).le φ → Le t T →
      Typ Ψ Γ (.await e) (.ex T φ)
  | raise : Typ Ψ Γ e (.ex (.errors ε) φ) → (Stage.rt Row.none ε).le φ →
      Typ Ψ Γ (.raise e) (.ex t φ)
  | raiseAdt : Typ Ψ Γ e (.ex (.adt E) φ) → (Stage.rt Row.none (Row.single E)).le φ →
      Typ Ψ Γ (.raise e) (.ex t φ)
  | errVal : Value e → Typ Ψ Γ e (.ex (.adt E) φ) → ε E → Typ Ψ Γ e (.ex (.errors ε) φ)
  | tryR : Typ Ψ Γ e (.ex t (.rt r ε₀)) → Typ Ψ Γ A (.halts ε₀ H t φ) → (Stage.rt r ε').le φ →
      Row.Sub ε₀ (Row.union H ε') → Typ Ψ Γ (.try_ e A) (.ex t φ)
  | tryP : Typ Ψ Γ e (.ex t .pure) → Typ Ψ Γ A (.halts Row.none H t φ) →
      Typ Ψ Γ (.try_ e A) (.ex t φ)
  | handle : Value v → Typ Ψ Γ v (.ex (.errors ε₀) .pure) → Typ Ψ Γ A (.halts ε₀ H t φ) →
      HCov S v H ε' → (Stage.rt Row.none ε').le φ → Typ Ψ Γ (.handle v A) (.ex t φ)
  | hnil : (∀ n, ¬ H n) → Typ Ψ Γ .anil (.halts ε₀ H t φ)
  | hcons : HPat S ε₀ p θ Δ → Typ Ψ (Δ.map .mono ++ Γ) b (.ex t φ) → Typ Ψ Γ r (.halts ε₀ H' t φ) →
      (∀ n, H n → Catches S ε₀ p θ n ∨ H' n) → Typ Ψ Γ (.acons p b r) (.halts ε₀ H t φ)
  | hguard : HPat S ε₀ p θ Δ → Typ Ψ (Δ.map .mono ++ Γ) g (.ex (.base .bool) φ) →
      Typ Ψ (Δ.map .mono ++ Γ) b (.ex t φ) → Typ Ψ Γ r (.halts ε₀ H' t φ) → Row.Sub H H' →
      Typ Ψ Γ (.aguard p g b r) (.halts ε₀ H t φ)
  | provideL : Typ Ψ Γ l (.ex (.layer i ε₂ o) φ) → Typ Ψ Γ e (.ex t φ₀) →
      φ₀.le (.rt (Row.union o r) ε) → (Stage.rt (Row.union i r) (Row.union ε₂ ε)).le φ →
      Typ Ψ Γ (.provide l e) (.ex t φ)
  | provideE : Typ Ψ Γ l (.ex (.env o) φ) → Typ Ψ Γ e (.ex t φ₀) →
      φ₀.le (.rt (Row.union o r) ε) → (Stage.rt r ε).le φ → Typ Ψ Γ (.provide l e) (.ex t φ)
  | with_ : Value θ → Typ Ψ Γ θ (.ex (.env ρ) .pure) → Typ Ψ Γ e (.ex t φ₀) →
      φ₀.le (.rt (Row.union ρ r) ε) → (Stage.rt r ε).le φ → Typ Ψ Γ (.with_ θ e) (.ex t φ)
  | enil : (∀ n, ¬ ρ n) → Typ Ψ Γ .enil (.ex (.env ρ) φ)
  | econs : Value v → IsEnv θ → Typ Ψ Γ v (.ex (S.impl N) φ) → Typ Ψ Γ θ (.ex (.env ρ) φ) →
      (∀ n, ρ' n → n = N ∨ ρ n) → Typ Ψ Γ (.econs N v θ) (.ex (.env ρ') φ)
  | bind : Typ Ψ Γ e (.ex (S.impl N) φ) → (∀ n, ρ n → n = N) →
      Typ Ψ Γ (.bind N e) (.ex (.env ρ) φ)
  | join : Typ Ψ Γ e₁ (.ex (.env ρ₁) φ) → Typ Ψ Γ e₂ (.ex (.env ρ₂) φ) →
      (∀ n, ρ n → ρ₁ n ∨ ρ₂ n) → Typ Ψ Γ (.join e₁ e₂) (.ex (.env ρ) φ)

/-- `⊢ μ : Ψ` of core.md §3.6. -/
def StoreTy (Ψ : List Ty) (μ : List Tm) : Prop :=
  μ.length = Ψ.length ∧
    ∀ (i : Nat) v t, μ[i]? = some v → Ψ[i]? = some t → Value v ∧ Typ S Ψ [] v (.ex t .pure)

/-- The root environment `Σ₀` of core.md §2.6: closed values of `S.Impl`. -/
def RootTy (root : Name → Option Tm) : Prop :=
  ∀ N v, root N = some v → Value v ∧ Typ S [] [] v (.ex (S.impl N) .pure)

end Flick
