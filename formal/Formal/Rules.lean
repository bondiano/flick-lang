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
`ε₀` is the error row of the body and `H` the errors the arms remove. The fields of a
dictionary record have the schemes `fs`. -/
inductive J
  | ex (t : Ty) (φ : Stage)
  | alts (t t' : Ty) (φ : Stage)
  | halts (ε₀ H : Row) (t : Ty) (φ : Stage)
  | dfs (fs : List (Label × Scheme))

/-- What a `with` frame of the `provide` region `q` with the services `ρ` gives its
body (regions.md §2.7): the frame answers a service of `ρ`, and the row outside records
the use `need N q`; it discharges an obligation `Provided {N} ϱ` for a service of `ρ`
when `q` outlives `ϱ`; everything else comes from outside. -/
def Row.withP (Θ : Facts) (q : Nat) (ρ r : Row) : Row
  | .nm N => (ρ (.nm N) → r (.need N (.r q))) ∧ (¬ ρ (.nm N) → r (.nm N))
  | .ob N a => (ρ (.nm N) → Outl Θ (· = .r q) a.ty) ∧ (¬ ρ (.nm N) → r (.ob N a))
  | x => r x

/-- The obligations `Provided {N} a` that the end of the region `a` discharges: `N` comes
from outside the region, whose provider outlives it (regions.md §2.7, second case). -/
def Row.disch (a : Atom) (r : Row) : Row
  | .ob N b => b = a ∧ r (.nm N)
  | _ => False

/-- The uses of the services of the `provide` region `a`. -/
def Row.needs (a : Atom) : Row
  | .need _ b => b = a
  | _ => False

theorem Stage.le_rt_sub {φ : Stage} {r r' ε : Row} (h : φ.le (.rt r ε)) (hr : Row.Sub r r') :
    φ.le (.rt r' ε) := by
  cases φ with
  | pure => trivial
  | rt r₀ e₀ => exact ⟨fun n hn => hr n (h.1 n hn), h.2⟩

theorem Stage.le_of_svc {φ : Stage} {r r' ε : Row} (h : φ.le (.rt r ε)) (hr : ∀ x, φ.svc x → r' x) :
    φ.le (.rt r' ε) := by
  cases φ with
  | pure => trivial
  | rt r₀ e₀ => exact ⟨hr, h.2⟩

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
      HCov S v H ε' → (∀ x, ε₀ x → (∃ N, x = .nm N) ∨ ε' x) → (Stage.rt Row.none ε').le φ →
      Typ Ψ Θ Γ (.handle v A) (.ex t φ)
  | hnil : (∀ n, ¬ H n) → Typ Ψ Θ Γ .anil (.halts ε₀ H t φ)
  | hcons : HPat S ε₀ p θ Δ → Typ Ψ Θ (Δ.map .mono ++ Γ) b (.ex t φ) →
      Typ Ψ Θ Γ r (.halts ε₀ H' t φ) → (∀ n, H n → Catches S ε₀ p θ n ∨ H' n) →
      Typ Ψ Θ Γ (.acons p b r) (.halts ε₀ H t φ)
  | hguard : HPat S ε₀ p θ Δ → Typ Ψ Θ (Δ.map .mono ++ Γ) g (.ex (.base .bool) φ) →
      Typ Ψ Θ (Δ.map .mono ++ Γ) b (.ex t φ) → Typ Ψ Θ Γ r (.halts ε₀ H' t φ) → Row.Sub H H' →
      Typ Ψ Θ Γ (.aguard p g b r) (.halts ε₀ H t φ)
  | provideL : Typ Ψ Θ Γ l (.ex (.layer i ε₂ o) φ) → Typ Ψ Θ Γ e (.ex t φ₀) →
      φ₀.le (.rt (Row.union o r) ε) → (o.Named ∧ ∀ N a, φ₀.svc (.ob N a) → ¬ o (.nm N)) →
      (Stage.rt (Row.union i r) (Row.union ε₂ ε)).le φ → Typ Ψ Θ Γ (.provide l e) (.ex t φ)
  | provideE : Typ Ψ Θ Γ l (.ex (.env o) φ) → Typ Ψ Θ Γ e (.ex t φ₀) →
      φ₀.le (.rt (Row.union o r) ε) → (o.Named ∧ ∀ N a, φ₀.svc (.ob N a) → ¬ o (.nm N)) →
      (Stage.rt r ε).le φ → Typ Ψ Θ Γ (.provide l e) (.ex t φ)
  | with_ : Value θ → Typ Ψ Θ Γ θ (.ex (.env ρ) .pure) → Typ Ψ Θ Γ e (.ex t φ₀) →
      φ₀.le (.rt (Row.withP Θ q ρ r) ε) → (Stage.rt r ε).le φ → Typ Ψ Θ Γ (.with_ q θ e) (.ex t φ)
  | enil : (∀ n, ¬ ρ n) → Typ Ψ Θ Γ .enil (.ex (.env ρ) φ)
  | econs : Value v → IsEnv θ → Typ Ψ Θ Γ v (.ex (S.impl N) φ) → Typ Ψ Θ Γ θ (.ex (.env ρ) φ) →
      (∀ n, ρ' n ↔ n = .nm N ∨ ρ n) → Typ Ψ Θ Γ (.econs N v θ) (.ex (.env ρ') φ)
  | bind : Typ Ψ Θ Γ e (.ex (S.impl N) φ) → (∀ n, ρ n ↔ n = .nm N) →
      Typ Ψ Θ Γ (.bind N e) (.ex (.env ρ) φ)
  | join : Typ Ψ Θ Γ e₁ (.ex (.env ρ₁) φ) → Typ Ψ Θ Γ e₂ (.ex (.env ρ₂) φ) →
      (∀ n, ρ n ↔ ρ₁ n ∨ ρ₂ n) → Typ Ψ Θ Γ (.join e₁ e₂) (.ex (.env ρ) φ)
  | scope : e.NoIns → (∀ C s, F C s → s = .tvar 0 ∧ ¬ C (.v 0)) →
      Typ (Ψ.map (Ty.up 1)) (Facts.union (Θ.substT (shiftV 1)) F)
        (.mono (.owner (.tvar 0)) :: Γ.map (Scheme.up 1)) e (.ex (t.up 1) φ') →
      φ'.le (.rt (Row.union (Row.scope (.tvar 0))
        (Row.union (r.substT (shiftV 1)) (Row.disch (.v 0) (r.substT (shiftV 1))))) (ε.substT (shiftV 1))) →
      (Stage.rt r ε).le φ → Le t T → Typ Ψ Θ Γ (.scope e) (.ex T φ)
  | inside : (∀ C s, F C s → s = .rg p ∧ ∀ q, C (.r q) → q < p) →
      Typ Ψ (Facts.union Θ F) Γ e (.ex t φ') →
      φ'.le (.rt (Row.union (Row.scope (.rg p)) (Row.union r (Row.union (Row.disch (.r p) r) (Row.needs (.r p))))) ε) →
      ¬ t.reach (.r p) → ¬ (Ty.errors ε).reach (.r p) → (Stage.rt r ε).le φ → Le t T →
      Typ Ψ Θ Γ (.inside p e) (.ex T φ)
  | cap : Typ Ψ Θ Γ (.cap p) (.ex (.hnd (.rg p)) φ)
  | own : Typ Ψ Θ Γ (.own p) (.ex (.owner (.rg p)) φ)
  | drec {us : List Ty} {fs : List (Label × Scheme)} : S.dict c = some (k, fs) → us.length = k →
      Typ Ψ Θ Γ r (.dfs (fs.map fun f => (f.1, f.2.substT (instT us)))) → r.NoIns →
      Caps.Sub (held Ψ Γ (.drec c r)) (Ty.dict c (Ty.spine us)).reach →
      Typ Ψ Θ Γ (.drec c r) (.ex (.dict c (Ty.spine us)) φ)
  | dnil : Typ Ψ Θ Γ .rnil (.dfs [])
  | dcons {s : Scheme} {fs : List (Label × Scheme)} : Typ (Ψ.map (Ty.up s.k)) (Facts.union (Θ.substT (shiftV s.k)) s.cs) (Γ.map (Scheme.up s.k)) v
        (.ex s.t .pure) →
      Typ Ψ Θ Γ r (.dfs fs) → Typ Ψ Θ Γ (.rcons l v r) (.dfs ((l, s) :: fs))
  | dproj {ts us : List Ty} {fs : List (Label × Scheme)} {s : Scheme} : Typ Ψ Θ Γ e (.ex (.dict c (Ty.spine us)) φ) → S.dict c = some (k, fs) →
      us.length = k → fs.lookup l = some s → ts.length = s.k → (s.substT (instT us)).Holds Θ ts →
      Le ((s.substT (instT us)).inst ts) T → Typ Ψ Θ Γ (.proj e l) (.ex T φ)
  | glob {ts : List Ty} : S.glob g = some σ → ts.length = σ.k → σ.Holds Θ ts → Le (σ.inst ts) T →
      Typ Ψ Θ Γ (.glob g) (.ex T φ)

/-- The facts of a running configuration: a region outlives every region created
after it. A scope gives its region facts about everything outside it, which
exists before the region; with the regions not yet created renamed out, they hold
of this order (regions.md §3.4). -/
def Facts.age : Facts := fun C s => ∀ q p, C (.r q) → s.reach (.r p) → q < p

/-- `⊢ μ : Ψ` of core.md §3.6. -/
def StoreTy (Ψ : List Ty) (μ : List Tm) : Prop :=
  μ.length = Ψ.length ∧
    ∀ (i : Nat) v t, μ[i]? = some v → Ψ[i]? = some t →
      Value v ∧ Typ S Ψ Facts.age [] v (.ex t .pure)

/-- The root environment `Σ₀` of core.md §2.6 and the bodies of the globals of λFlick⁺. -/
structure Roots where
  svc : Name → Option Tm
  glob : Nat → Option Tm

/-- `Σ₀` has closed values of `S.Impl`. The body of a global is closed source code,
typed at the scheme of the global; each declared global has a body. -/
structure RootTy (root : Roots) : Prop where
  svc : ∀ N v, root.svc N = some v → Value v ∧ Typ S [] Facts.none [] v (.ex (S.impl N) .pure)
  glob : ∀ g v, root.glob g = some v → (∀ x, ¬ v.Has x) ∧
    ∃ s, S.glob g = some s ∧ Typ S [] s.cs [] v (.ex s.t .pure)
  globs : ∀ g s, S.glob g = some s → (root.glob g).isSome

end Flick
