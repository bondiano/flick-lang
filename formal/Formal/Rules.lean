import Formal.EvalCtx

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
dictionary record have the schemes `fs`. The arms of a handler of the effect `E` give
the handler's type `t` at its stage `φ`; `resume` holds the region `s` of the handler. -/
inductive J
  | ex (t : Ty) (φ : Stage)
  | alts (t t' : Ty) (φ : Stage)
  | halts (ε₀ H : Row) (t : Ty) (φ : Stage)
  | dfs (fs : List (Label × Scheme))
  | harms (E : Name) (t : Ty) (φ : Stage) (s : Ty)

/-- What a handler of `E` of the region `q` gives its body (regions.md §2.7): the frame
answers `E`, and it discharges an obligation `Provided {E} ϱ` when `q` outlives `ϱ`;
everything else comes from outside. -/
def Row.hP (Θ : Facts) (q : Nat) (E : Name) (r : Row) : Row
  | .nm N => N = E ∨ r (.nm N)
  | .ob N a => (N = E → Outl Θ (· = .r q) a.ty) ∧ (N ≠ E → r (.ob N a))
  | x => r x

/-- No obligation of the stage `φ₀` of the body of a handler is about an effect it may
give (regions.md §2.7, first case, rejected): a named obligation needs `o` without row
variables and without its effect, one over a row variable needs `o` without effects. -/
def Stage.ObOut (φ₀ : Stage) (o : Row) : Prop :=
  (∀ N a, φ₀.svc (.ob N a) → o.Named ∧ ¬ o (.nm N)) ∧
  ∀ v a, φ₀.svc (.obv v a) → o.Named ∧ ∀ N, ¬ o (.nm N)

/-- The obligations `Provided {N} a` that the end of the region `a` discharges: `N` comes
from outside the region, whose handler outlives it (regions.md §2.7, second case); and so
does `Provided v a` for a row variable `v` of the row outside. -/
def Row.disch (a : Atom) (r : Row) : Row
  | .ob N b => b = a ∧ r (.nm N)
  | .obv v b => b = a ∧ r (.rv v)
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

/-- A body with no obligation about the effect of its handler has what the handler gives. -/
theorem Stage.hP_of_out {φ₀ : Stage} {E : Name} {r ε : Row} {Θ : Facts} {q : Nat}
    (h : φ₀.le (.rt (Row.union (Row.single E) r) ε)) (hO : φ₀.ObOut (Row.single E)) :
    φ₀.le (.rt (Row.hP Θ q E r) ε) := by
  cases φ₀ with
  | pure => trivial
  | rt r₀ e₀ =>
    refine ⟨fun x hx => ?_, h.2⟩
    have h₁ := h.1 x hx
    cases x with
    | nm N =>
      rcases h₁ with h₁ | h₁
      · cases h₁; exact .inl rfl
      · exact .inr h₁
    | ob N a =>
      refine ⟨fun hN => ((hO.1 N a hx).2 (by rw [hN]; rfl)).elim, fun _ => ?_⟩
      rcases h₁ with h₁ | h₁
      · cases h₁
      · exact h₁
    | atm a | rv a | obv v a =>
      rcases h₁ with h₁ | h₁
      · cases h₁
      · exact h₁

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
      Sig.Holds ts T₀ (S.res K) us → Le (Sig.ctorTyR ts T₀ (S.res K) us) T →
      Typ Ψ Θ Γ (.ctor K) (.ex T φ)
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
  /-- An arm on a constructor `K` of a GADT, which builds `T rs` over its `k`
  variables, with the patterns `qs` of its fields: under each instance `θ` of those
  variables and of the ones around that makes `T rs` the matched type and fits the
  arm, the fields' patterns bind `Δ θ` and the arm has the result's type. No such
  `θ`: no value of the matched type is built by `K`, and the arm is dead. A dead arm
  still names only the variables and cells around it. The matched type is a type
  `T …`, as for the patterns of ordinary constructors. -/
  | aref {qs : List Pat} {ts : List Ty} {Δ : (Atom → Ty) → List Ty} :
      S.ctor K = some (k, ts, T₀) → t.hd = .adt T₀ → b.NoIns →
      (∀ i, b.Free i → i < (Pat.spine K qs).binds + Γ.length) →
      (∀ l, b.Has (.loc l) → l < Ψ.length) →
      (∀ θ, Fits θ b → (Ty.tcon T₀ (S.res K)).substT θ = (t.up k).substT θ →
        PatsTy S qs (ts.map (Ty.substT θ)) (Δ θ)) →
      (∀ θ, Fits θ b → (Ty.tcon T₀ (S.res K)).substT θ = (t.up k).substT θ →
        Typ ((Ψ.map (Ty.up k)).map (Ty.substT θ)) ((Θ.substT (shiftV k)).substT θ)
          ((Δ θ).map .mono ++ (Γ.map (Scheme.up k)).map (Scheme.substT θ)) b
          (.ex ((t'.up k).substT θ) ((φ.substT (shiftV k)).substT θ))) →
      Typ Ψ Θ Γ r (.alts t t' φ) → Typ Ψ Θ Γ (.acons (Pat.spine K qs) b r) (.alts t t' φ)
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
      HCov S v H ε' → (∀ x, ε₀ x → (∃ N, x = .nm N) ∨ Row.union H ε' x) →
      (Stage.rt Row.none ε').le φ →
      Typ Ψ Θ Γ (.handle v A) (.ex t φ)
  | hnil : (∀ n, ¬ H n) → Typ Ψ Θ Γ .anil (.halts ε₀ H t φ)
  | hcons : HPat S ε₀ p θ Δ → Typ Ψ Θ (Δ.map .mono ++ Γ) b (.ex t φ) →
      Typ Ψ Θ Γ r (.halts ε₀ H' t φ) → (∀ n, H n → Catches S ε₀ p θ n ∨ H' n) →
      Typ Ψ Θ Γ (.acons p b r) (.halts ε₀ H t φ)
  | hguard : HPat S ε₀ p θ Δ → Typ Ψ Θ (Δ.map .mono ++ Γ) g (.ex (.base .bool) φ) →
      Typ Ψ Θ (Δ.map .mono ++ Γ) b (.ex t φ) → Typ Ψ Θ Γ r (.halts ε₀ H' t φ) → Row.Sub H H' →
      Typ Ψ Θ Γ (.aguard p g b r) (.halts ε₀ H t φ)
  | scope : e.NoIns → (∀ C s, F C s → s = .tvar 0 ∧ ¬ C (.v 0)) →
      Typ (Ψ.map (Ty.up 1)) (Facts.union (Θ.substT (shiftV 1)) F)
        (.mono (.owner (.tvar 0)) :: Γ.map (Scheme.up 1)) e (.ex (t.up 1) φ') →
      φ'.le (.rt (Row.union (Row.scope (.tvar 0))
        (Row.union (r.substT (shiftV 1)) (Row.disch (.v 0) (r.substT (shiftV 1))))) (ε.substT (shiftV 1))) →
      (Stage.rt r ε).le φ → Le t T → Typ Ψ Θ Γ (.scope e) (.ex T φ)
  | inside : (∀ C s, F C s → s = .rg p ∧ ∀ q, C (.r q) → q < p) →
      Typ Ψ (Facts.union Θ F) Γ e (.ex t φ') →
      φ'.le (.rt (Row.union (Row.scope (.rg p)) (Row.union r (Row.disch (.r p) r))) ε) →
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
  /-- An operation of a declared effect: its value needs the effect; an `abort`
  operation never returns, so its result has any type. -/
  | perform : S.op E o = some (c, a, b) →
      Le (.arrow a (.rt (Row.single E) Row.none) (if c = .abort then t else b) Caps.none) T →
      Typ Ψ Θ Γ (.const (.perform E o c)) (.ex T φ)
  /-- The arms of a handler, `x` the operation's argument and, for `control`, `k` its
  `resume`, which runs the rest of the body under the handler and holds its region. -/
  | hanil : Typ Ψ Θ Γ .anil (.harms E t φ s)
  | hatail : S.op E o = some (.tail, a, b) → Typ Ψ Θ (.mono a :: Γ) e (.ex b φ) →
      Typ Ψ Θ Γ r (.harms E t φ s) → Typ Ψ Θ Γ (.acons (Pat.spine o [.var]) e r) (.harms E t φ s)
  | haabort : S.op E o = some (.abort, a, b) → Typ Ψ Θ (.mono a :: Γ) e (.ex t φ) →
      Typ Ψ Θ Γ r (.harms E t φ s) → Typ Ψ Θ Γ (.acons (Pat.spine o [.var]) e r) (.harms E t φ s)
  | hactl : S.op E o = some (.control, a, b) →
      Typ Ψ Θ (.mono a :: .mono (.arrow b φ t s.reach) :: Γ) e (.ex t φ) →
      Typ Ψ Θ Γ r (.harms E t φ s) → Typ Ψ Θ Γ (.acons (Pat.spine o [.var, .var]) e r) (.harms E t φ s)
  /-- (Handle): the body may perform `E`; the arms run at the handler's stage, `resume`
  over a region variable of the handler, as the region of a `scope`; `rf` makes the
  handler's value of the body's. Without an arm for every operation, the body's other
  operations of `E` go on to the handler around. -/
  | hdl {al rf : Tm} : Typ (Ψ.map (Ty.up 1)) (Θ.substT (shiftV 1)) (Γ.map (Scheme.up 1)) al
        (.harms E (t'.up 1) ((Stage.rt r ε).substT (shiftV 1)) (.tvar 0)) → al.NoIns →
      Typ Ψ Θ Γ rf (.ex (.arrow t ψ t' C) (.rt r ε)) → ψ.le (.rt r ε) →
      Typ Ψ Θ Γ e (.ex t φ₀) → φ₀.le (.rt (Row.union (Row.single E) r) ε) → φ₀.ObOut (Row.single E) →
      S.Handles E al r → (Stage.rt r ε).le φ → Le t' T →
      Typ Ψ Θ Γ (.hdl E al rf e) (.ex T φ)
  /-- The run-time forms of a handler, whose region is `p`; the cell `ℓ` holds the type and
  the stage of its value. -/
  | hwith {al rf : Tm} : Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) →
      Typ Ψ Θ Γ al (.harms E t' (.rt r ε) (.rg p)) →
      Typ Ψ Θ Γ rf (.ex (.arrow t ψ t' C) (.rt r ε)) → ψ.le (.rt r ε) →
      Typ Ψ Θ Γ e (.ex t φ₀) → φ₀.le (.rt (Row.hP Θ p E r) ε) →
      S.Handles E al r → (Stage.rt r ε).le φ → Le t' T →
      Typ Ψ Θ Γ (.hwith p ℓ E al rf e) (.ex T φ)
  /-- While an arm runs, the body is parked where it performed the operation `o`: it is
  typed with the operation at the hole, the argument left out. A `tail` arm gives that
  operation its value, a `control` arm the handler its value, with `resume` the cell `κ`. -/
  | hctl {Ei : List Frame} {al rf : Tm} {tl : Bool} :
      Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) →
      Typ Ψ Θ Γ al (.harms E t' (.rt r ε) (.rg p)) →
      Typ Ψ Θ Γ rf (.ex (.arrow t ψ t' C) (.rt r ε)) → ψ.le (.rt r ε) →
      S.op E o = some (c, a₀, b₀) → c ≠ .abort → (tl = true ↔ c = .tail) →
      OkE Ei → k = plug Ei (.hole p) → Typ Ψ Θ [] (plug Ei (.app (.const (.perform E o c)) .todo)) (.ex t φ₀) →
      φ₀.le (.rt (Row.hP Θ p E r) ε) →
      Typ Ψ Θ Γ a (.ex (if tl then b₀ else t') (.rt r ε)) →
      (tl = false → ∃ C', Ψ[κ]? = some (.arrow b₀ (.rt r ε) t' C') ∧ C' (.r p)) →
      S.Handles E al r → (Stage.rt r ε).le φ → Le t' T →
      Typ Ψ Θ Γ (.hctl p ℓ κ tl E al rf a k) (.ex T φ)
  /-- While the resumed body runs, the arm is parked where it called `resume`, the
  argument left out. -/
  | hres {A : List Frame} : Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) →
      Typ Ψ Θ Γ b (.ex t' (.rt r ε)) →
      OkE A → a = plug A (.hole p) → Typ Ψ Θ [] (plug A (.app (.kont p κ) .todo)) (.ex t' (.rt r ε)) →
      Ψ[κ]? = some (.arrow b₀ (.rt r ε) t' C') → (Stage.rt r ε).le φ → Le t' T →
      Typ Ψ Θ Γ (.hres p ℓ a b) (.ex T φ)
  | hdone : Typ Ψ Θ Γ a (.ex t φ) → Typ Ψ Θ Γ (.hdone p a) (.ex t φ)
  /-- Unwinding to the handler of the cell `ℓ`, which then runs `a` with the type of its
  value; unwinding itself has any type. -/
  | unw : Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) → Typ Ψ Θ Γ a (.ex t' (.rt r ε)) →
      (Stage.rt Row.none Row.none).le φ → Typ Ψ Θ Γ (.unw p ℓ a) (.ex t φ)
  /-- `resume`: the type of its cell, which holds the handler's region. -/
  | kont : Ψ[ℓ]? = some (.arrow a ψ t C) → C (.r p) → Le (.arrow a ψ t C) T →
      Typ Ψ Θ Γ (.kont p ℓ) (.ex T φ)

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

/-- An ordinary constructor at `T us`. -/
theorem Typ.ctor_plain {Ψ Θ Γ K k ts T₀ us T φ} (hK : S.ctor K = some (k, ts, T₀))
    (hres : S.res K = idTs k) (hk : us.length = k) (hT : Le (Sig.ctorTy ts T₀ us) T) :
    Typ S Ψ Θ Γ (.ctor K) (.ex T φ) :=
  .ctor hK hk (S.holds_plain hK hres hk) (by rw [Sig.ctorTyR_plain hres hk]; exact hT)

end Flick
