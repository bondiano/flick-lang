import Formal.EvalCtx
import Formal.RuleRows
import Formal.Live.Mode

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

variable (S : Sig) (mode : TypingMode)

inductive TypM : List Ty → Facts → List Scheme → Tm → J → Prop
  | var {ts : List Ty} : Γ[i]? = some σ → ts.length = σ.k → σ.Holds Θ ts → mode.instSubtype (σ.inst ts) T →
      TypM Ψ Θ Γ (.var i) (.ex T φ)
  /-- A value of a polymorphic type, typed under `k` fresh variables as in (Let-Gen). -/
  | allI : Value e → TypM (Ψ.map (Ty.up k)) (Θ.substT (shiftV k)) (Γ.map (Scheme.up k)) e (.ex t .pure) →
      TypM Ψ Θ Γ e (.ex (.all k t) φ)
  /-- A variable of a scheme without facts, as a value of its polymorphic type. -/
  | allVar : Γ[i]? = some σ → 0 < σ.k → (∀ C s, ¬ σ.cs C s) → TypM Ψ Θ Γ (.var i) (.ex (.all σ.k σ.t) φ)
  | lam : TypM Ψ Θ (.mono a :: Γ) b (.ex t (mode.use ψ C)) → b.NoIns →
      mode.held (held Ψ Γ (.lam b)) (Ty.arrow a ψ t C).reach → mode.subtype (.arrow a ψ t C) T →
      TypM Ψ Θ Γ (.lam b) (.ex T φ)
  | app : TypM Ψ Θ Γ f (.ex (.arrow a ψ t C) φ) → TypM Ψ Θ Γ e (.ex a φ) → (mode.use ψ C).le φ → mode.subtype t T →
      TypM Ψ Θ Γ (.app f e) (.ex T φ)
  | let_ : TypM Ψ Θ Γ e (.ex a φ) → TypM Ψ Θ (.mono a :: Γ) b (.ex t φ) →
      TypM Ψ Θ Γ (.let_ e b) (.ex t φ)
  | letGen : TypM (Ψ.map (Ty.up k)) (Facts.union (Θ.substT (shiftV k)) Φ) (Γ.map (Scheme.up k)) e
        (.ex a .pure) →
      TypM Ψ Θ (⟨k, Φ, a⟩ :: Γ) b (.ex t φ) → TypM Ψ Θ Γ (.let_ e b) (.ex t φ)
  | letrec : TypM Ψ Θ (.mono a :: .mono (.arrow a ψ r C) :: Γ) f (.ex r (mode.use ψ C)) → f.NoIns →
      mode.held (held Ψ Γ (.fix f)) (Ty.arrow a ψ r C).reach →
      TypM Ψ Θ (.mono (.arrow a ψ r C) :: Γ) b (.ex t φ) → TypM Ψ Θ Γ (.letrec f b) (.ex t φ)
  | fix : TypM Ψ Θ (.mono a :: .mono (.arrow a ψ r C) :: Γ) f (.ex r (mode.use ψ C)) → f.NoIns →
      mode.held (held Ψ Γ (.fix f)) (Ty.arrow a ψ r C).reach → mode.subtype (.arrow a ψ r C) T →
      TypM Ψ Θ Γ (.fix f) (.ex T φ)
  | lit : TypM Ψ Θ Γ (.lit l) (.ex (.base l.base) φ)
  | loc : Ψ[l]? = some t → TypM Ψ Θ Γ (.loc l) (.ex (.ref t) φ)
  | const : c.ok Θ ι mode → mode.subtype (c.ty S ι mode) T → TypM Ψ Θ Γ (.const c) (.ex T φ)
  | ctor {ts us : List Ty} : S.ctor K = some (k, ts, T₀) → us.length = k →
      Sig.Holds ts T₀ (S.res K) us → mode.subtype (Sig.ctorTyR ts T₀ (S.res K) us) T →
      TypM Ψ Θ Γ (.ctor K) (.ex T φ)
  | rnil : TypM Ψ Θ Γ .rnil (.ex .rnil φ)
  | rcons : R.IsRec → l ∉ R.labels → TypM Ψ Θ Γ e (.ex t φ) → TypM Ψ Θ Γ r (.ex R φ) →
      TypM Ψ Θ Γ (.rcons l e r) (.ex (.rcons l t R) φ)
  | proj : TypM Ψ Θ Γ e (.ex R φ) → R.field l = some t → mode.subtype t T → TypM Ψ Θ Γ (.proj e l) (.ex T φ)
  | update : TypM Ψ Θ Γ e (.ex R φ) → R.field l = some t → TypM Ψ Θ Γ e' (.ex t φ) →
      TypM Ψ Θ Γ (.update e l e') (.ex R φ)
  | match_ : TypM Ψ Θ Γ e (.ex t φ) → TypM Ψ Θ Γ A (.alts t t' φ) → Covers S t A.unguarded e →
      TypM Ψ Θ Γ (.match_ e A) (.ex t' φ)
  | anil : TypM Ψ Θ Γ .anil (.alts t t' φ)
  | acons : PatTy S true p t Δ → TypM Ψ Θ (Δ.map .mono ++ Γ) b (.ex t' φ) →
      TypM Ψ Θ Γ r (.alts t t' φ) → TypM Ψ Θ Γ (.acons p b r) (.alts t t' φ)
  | aguard : PatTy S true p t Δ → TypM Ψ Θ (Δ.map .mono ++ Γ) g (.ex (.base .bool) φ) →
      TypM Ψ Θ (Δ.map .mono ++ Γ) b (.ex t' φ) → TypM Ψ Θ Γ r (.alts t t' φ) →
      TypM Ψ Θ Γ (.aguard p g b r) (.alts t t' φ)
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
        TypM ((Ψ.map (Ty.up k)).map (Ty.substT θ)) ((Θ.substT (shiftV k)).substT θ)
          ((Δ θ).map .mono ++ (Γ.map (Scheme.up k)).map (Scheme.substT θ)) b
          (.ex ((t'.up k).substT θ) ((φ.substT (shiftV k)).substT θ))) →
      TypM Ψ Θ Γ r (.alts t t' φ) → TypM Ψ Θ Γ (.acons (Pat.spine K qs) b r) (.alts t t' φ)
  | todo : TypM Ψ Θ Γ .todo (.ex t φ)
  | task : TypM Ψ Θ Γ e (.ex t ψ) → ψ.le (mode.use (.rt r ε) C) → e.NoIns →
      mode.held (held Ψ Γ (.task e)) (Ty.eff r ε t C).reach → mode.subtype (.eff r ε t C) T →
      TypM Ψ Θ Γ (.task e) (.ex T φ)
  | await : TypM Ψ Θ Γ e (.ex (.eff r ε t C) φ) → (mode.use (.rt r ε) C).le φ → mode.subtype t T →
      TypM Ψ Θ Γ (.await e) (.ex T φ)
  | raise : TypM Ψ Θ Γ e (.ex (.errors ε) φ) → (Stage.rt Row.none ε).le φ →
      TypM Ψ Θ Γ (.raise e) (.ex t φ)
  | raiseAdt : TypM Ψ Θ Γ e (.ex U φ) → U.hd = .adt E → (Stage.rt Row.none (Row.err E U)).le φ →
      TypM Ψ Θ Γ (.raise e) (.ex t φ)
  | errVal : Value e → TypM Ψ Θ Γ e (.ex U φ) → U.hd = .adt E → Row.Sub (Row.err E U) ε →
      TypM Ψ Θ Γ e (.ex (.errors ε) φ)
  | tryR : TypM Ψ Θ Γ e (.ex t (.rt r ε₀)) → TypM Ψ Θ Γ A (.halts ε₀ H t φ) →
      (Stage.rt r ε').le φ → Row.Sub ε₀ (Row.union H ε') → TypM Ψ Θ Γ (.try_ e A) (.ex t φ)
  | tryP : TypM Ψ Θ Γ e (.ex t .pure) → TypM Ψ Θ Γ A (.halts Row.none H t φ) →
      TypM Ψ Θ Γ (.try_ e A) (.ex t φ)
  | handle : Value v → TypM Ψ Θ Γ v (.ex (.errors ε₀) .pure) → TypM Ψ Θ Γ A (.halts ε₀ H t φ) →
      HCov S v H ε' → (∀ x, ε₀ x → (∃ N, x = .nm N) ∨ Row.union H ε' x) →
      (Stage.rt Row.none ε').le φ →
      TypM Ψ Θ Γ (.handle v A) (.ex t φ)
  | hnil : (∀ n, ¬ H n) → TypM Ψ Θ Γ .anil (.halts ε₀ H t φ)
  | hcons : HPat S ε₀ p θ Δ → TypM Ψ Θ (Δ.map .mono ++ Γ) b (.ex t φ) →
      TypM Ψ Θ Γ r (.halts ε₀ H' t φ) → (∀ n, H n → Catches S ε₀ p θ n ∨ H' n) →
      TypM Ψ Θ Γ (.acons p b r) (.halts ε₀ H t φ)
  | hguard : HPat S ε₀ p θ Δ → TypM Ψ Θ (Δ.map .mono ++ Γ) g (.ex (.base .bool) φ) →
      TypM Ψ Θ (Δ.map .mono ++ Γ) b (.ex t φ) → TypM Ψ Θ Γ r (.halts ε₀ H' t φ) → Row.Sub H H' →
      TypM Ψ Θ Γ (.aguard p g b r) (.halts ε₀ H t φ)
  | scope : e.NoIns → (∀ C s, F C s → s = .tvar 0 ∧ ¬ C (.v 0)) →
      TypM (Ψ.map (Ty.up 1)) (Facts.union (Θ.substT (shiftV 1)) F)
        (.mono (.owner (.tvar 0)) :: Γ.map (Scheme.up 1)) e (.ex (t.up 1) φ') →
      φ'.le (.rt (Row.union (Row.scope (.tvar 0))
        (Row.union (r.substT (shiftV 1)) (Row.disch (.v 0) (r.substT (shiftV 1))))) (ε.substT (shiftV 1))) →
      (Stage.rt r ε).le φ → mode.subtype t T → TypM Ψ Θ Γ (.scope e) (.ex T φ)
  | inside : (∀ C s, F C s → s = .rg p ∧ ∀ q, C (.r q) → q < p) →
      TypM Ψ (Facts.union Θ F) Γ e (.ex t φ') →
      φ'.le (.rt (Row.union (Row.scope (.rg p)) (Row.union r (Row.disch (.r p) r))) ε) →
      ¬ t.reach (.r p) → ¬ (Ty.errors ε).reach (.r p) → (Stage.rt r ε).le φ → mode.subtype t T →
      TypM Ψ Θ Γ (.inside p e) (.ex T φ)
  | cap : TypM Ψ Θ Γ (.cap p) (.ex (.hnd (.rg p)) φ)
  | own : TypM Ψ Θ Γ (.own p) (.ex (.owner (.rg p)) φ)
  | drec {us : List Ty} {fs : List (Label × Scheme)} : S.dict c = some (k, fs) → us.length = k →
      TypM Ψ Θ Γ r (.dfs (fs.map fun f => (f.1, f.2.substT (instT us)))) → r.NoIns →
      mode.held (held Ψ Γ (.drec c r)) (Ty.dict c (Ty.spine us)).reach →
      TypM Ψ Θ Γ (.drec c r) (.ex (.dict c (Ty.spine us)) φ)
  | dnil : TypM Ψ Θ Γ .rnil (.dfs [])
  | dcons {s : Scheme} {fs : List (Label × Scheme)} : TypM (Ψ.map (Ty.up s.k)) (Facts.union (Θ.substT (shiftV s.k)) s.cs) (Γ.map (Scheme.up s.k)) v
        (.ex s.t .pure) →
      TypM Ψ Θ Γ r (.dfs fs) → TypM Ψ Θ Γ (.rcons l v r) (.dfs ((l, s) :: fs))
  | dproj {ts us : List Ty} {fs : List (Label × Scheme)} {s : Scheme} : TypM Ψ Θ Γ e (.ex (.dict c (Ty.spine us)) φ) → S.dict c = some (k, fs) →
      us.length = k → fs.lookup l = some s → ts.length = s.k → (s.substT (instT us)).Holds Θ ts →
      mode.subtype ((s.substT (instT us)).inst ts) T → TypM Ψ Θ Γ (.proj e l) (.ex T φ)
  | glob {ts : List Ty} : S.glob g = some σ → ts.length = σ.k → σ.Holds Θ ts → mode.subtype (σ.inst ts) T →
      TypM Ψ Θ Γ (.glob g) (.ex T φ)
  /-- An operation of a declared effect: its value needs the effect; an `abort`
  operation never returns, so its result has any type. -/
  | perform : S.op E o = some (c, a, b) →
      mode.subtype (.arrow a (.rt (Row.single E) Row.none) (if c = .abort then t else b) Caps.none) T →
      TypM Ψ Θ Γ (.const (.perform E o c)) (.ex T φ)
  /-- The arms of a handler, `x` the operation's argument and, for `control`, `k` its
  `resume`, which runs the rest of the body under the handler and holds its region. -/
  | hanil : TypM Ψ Θ Γ .anil (.harms E t φ s)
  | hatail : S.op E o = some (.tail, a, b) → TypM Ψ Θ (.mono a :: Γ) e (.ex b φ) →
      TypM Ψ Θ Γ r (.harms E t φ s) → TypM Ψ Θ Γ (.acons (Pat.spine o [.var]) e r) (.harms E t φ s)
  | haabort : S.op E o = some (.abort, a, b) → TypM Ψ Θ (.mono a :: Γ) e (.ex t φ) →
      TypM Ψ Θ Γ r (.harms E t φ s) → TypM Ψ Θ Γ (.acons (Pat.spine o [.var]) e r) (.harms E t φ s)
  | hactl : S.op E o = some (.control, a, b) →
      -- The handler's own region is live while its control arm calls resume.
      TypM Ψ Θ (.mono a :: .mono (.arrow b φ t s.reach) :: Γ) e (.ex t (mode.use φ s.reach)) →
      TypM Ψ Θ Γ r (.harms E t φ s) → TypM Ψ Θ Γ (.acons (Pat.spine o [.var, .var]) e r) (.harms E t φ s)
  /-- (Handle): the body may perform `E`; the arms run at the handler's stage, `resume`
  over a region variable of the handler, as the region of a `scope`; `rf` makes the
  handler's value of the body's. Without an arm for every operation, the body's other
  operations of `E` go on to the handler around. -/
  | hdl {al rf : Tm} : TypM (Ψ.map (Ty.up 1)) (Θ.substT (shiftV 1)) (Γ.map (Scheme.up 1)) al
        (.harms E (t'.up 1) ((Stage.rt r ε).substT (shiftV 1)) (.tvar 0)) → al.NoIns →
      TypM Ψ Θ Γ rf (.ex (.arrow t ψ t' C) (.rt r ε)) → (mode.use ψ C).le (.rt r ε) →
      TypM Ψ Θ Γ e (.ex t φ₀) → φ₀.le (.rt (Row.union (Row.single E) r) ε) → φ₀.ObOut (Row.single E) →
      S.Handles E al r → (Stage.rt r ε).le φ → mode.subtype t' T →
      TypM Ψ Θ Γ (.hdl E al rf e) (.ex T φ)
  /-- The run-time forms of a handler, whose region is `p`; the cell `ℓ` holds the type and
  the stage of its value. -/
  | hwith {al rf : Tm} : Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) →
      TypM Ψ Θ Γ al (.harms E t' (.rt r ε) (.rg p)) →
      TypM Ψ Θ Γ rf (.ex (.arrow t ψ t' C) (.rt r ε)) → (mode.use ψ C).le (.rt r ε) →
      TypM Ψ Θ Γ e (.ex t φ₀) → φ₀.le (.rt (Row.hP Θ p E r) ε) →
      S.Handles E al r → (Stage.rt r ε).le φ → mode.subtype t' T →
      (mode = .live → φ.svc (.atm (.r p))) →
      TypM Ψ Θ Γ (.hwith p ℓ E al rf e) (.ex T φ)
  /-- While an arm runs, the body is parked where it performed the operation `o`: it is
  typed with the operation at the hole, the argument left out. A `tail` arm gives that
  operation its value, a `control` arm the handler its value, with `resume` the cell `κ`. -/
  | hctl {Ei : List Frame} {al rf : Tm} {tl : Bool} :
      Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) →
      TypM Ψ Θ Γ al (.harms E t' (.rt r ε) (.rg p)) →
      TypM Ψ Θ Γ rf (.ex (.arrow t ψ t' C) (.rt r ε)) → (mode.use ψ C).le (.rt r ε) →
      S.op E o = some (c, a₀, b₀) → c ≠ .abort → (tl = true ↔ c = .tail) →
      OkE Ei → k = plug Ei (.hole p) → TypM Ψ Θ [] (plug Ei (.app (.const (.perform E o c)) .todo)) (.ex t φ₀) →
      φ₀.le (.rt (Row.hP Θ p E r) ε) →
      TypM Ψ Θ Γ a (.ex (if tl then b₀ else t')
        (mode.use (.rt r ε) (if tl then Caps.none else (Ty.rg p).reach))) →
      (tl = false → ∃ C', Ψ[κ]? = some (.arrow b₀ (.rt r ε) t' C') ∧ C' (.r p)) →
      S.Handles E al r → (Stage.rt r ε).le φ → mode.subtype t' T →
      (mode = .live → φ.svc (.atm (.r p))) →
      TypM Ψ Θ Γ (.hctl p ℓ κ tl E al rf a k) (.ex T φ)
  /-- While the resumed body runs, the arm is parked where it called `resume`, the
  argument left out. -/
  | hres {A : List Frame} : Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) →
      TypM Ψ Θ Γ b (.ex t' (mode.use (.rt r ε) (Ty.rg p).reach)) →
      OkE A → a = plug A (.hole p) → TypM Ψ Θ [] (plug A (.app (.kont p κ) .todo))
        (.ex t' (mode.use (.rt r ε) (Ty.rg p).reach)) →
      Ψ[κ]? = some (.arrow b₀ (.rt r ε) t' C') → C' (.r p) → (Stage.rt r ε).le φ →
      mode.subtype t' T → TypM Ψ Θ Γ (.hres p ℓ a b) (.ex T φ)
  | hdone : TypM Ψ Θ Γ a (.ex t (mode.use φ (Ty.rg p).reach)) →
      TypM Ψ Θ Γ (.hdone p a) (.ex t φ)
  /-- Unwinding to the handler of the cell `ℓ`, which then runs `a` with the type of its
  value; unwinding itself has any type. -/
  | unw : Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) →
      TypM Ψ Θ Γ a (.ex t' (mode.use (.rt r ε) (Ty.rg p).reach)) →
      (Stage.rt Row.none Row.none).le φ → TypM Ψ Θ Γ (.unw p ℓ a) (.ex t φ)
  /-- A handler kept around a child: `ℓ` and the arms are those of the handler it keeps. The
  child runs the arms, so its stage holds theirs; an operation the kept handler does not answer
  goes on to the handler around, or the root. -/
  | hkeep {al : Tm} : Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) →
      TypM Ψ Θ Γ al (.harms E t' (.rt r ε) (.rg q)) →
      TypM Ψ Θ Γ e (.ex t φ₀) → φ₀.le (.rt (Row.hP Θ q E r₁) ε₁) →
      S.Handles E al r₁ → S.NonTail E r₁ → (Stage.rt r ε).le (.rt r₁ ε₁) →
      (Stage.rt r₁ ε₁).le φ → mode.subtype t T → TypM Ψ Θ Γ (.hkeep q ℓ E al e) (.ex T φ)
  /-- While an arm of a kept handler runs, the body is parked where it performed the operation
  `o`, as for `hctl` of a `tail` operation. -/
  | hkarm {Ei : List Frame} {al : Tm} : Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none) →
      TypM Ψ Θ Γ al (.harms E t' (.rt r ε) (.rg q)) →
      S.op E o = some (.tail, a₀, b₀) → OkE Ei → k = plug Ei (.hole q) →
      TypM Ψ Θ [] (plug Ei (.app (.const (.perform E o .tail)) .todo)) (.ex t φ₀) →
      φ₀.le (.rt (Row.hP Θ q E r₁) ε₁) → S.Handles E al r₁ → S.NonTail E r₁ →
      TypM Ψ Θ Γ a (.ex b₀ (.rt r ε)) → (Stage.rt r ε).le (.rt r₁ ε₁) →
      (Stage.rt r₁ ε₁).le φ → mode.subtype t T → TypM Ψ Θ Γ (.hkarm q ℓ E al a k) (.ex T φ)
  /-- `resume`: the type of its cell, which holds the handler's region. -/
  | kont : Ψ[ℓ]? = some (.arrow a ψ t C) → C (.r p) → mode.subtype (.arrow a ψ t C) T →
      TypM Ψ Θ Γ (.kont p ℓ) (.ex T φ)

variable {mode : TypingMode}

/-- The existing capture discipline, with its original rules definitionally unchanged. -/
abbrev Typ := TypM S .captures

/-- The discipline where runtime use, rather than holding a value, needs a live region. -/
abbrev TypLive := TypM S .live

namespace Typ
-- Keep constructor names and field notation for the existing captures proofs.
export TypM (
  var allI allVar lam app let_ letGen letrec fix lit loc const ctor rnil rcons proj update
  match_ anil acons aguard aref todo task await raise raiseAdt errVal tryR tryP handle
  hnil hcons hguard scope inside cap own drec dnil dcons dproj glob perform hanil
  hatail haabort hactl hdl hwith hctl hres hdone unw hkeep hkarm kont
)
end Typ

/-- The facts of a running configuration: a region outlives every region created
after it. A scope gives its region facts about everything outside it, which
exists before the region; with the regions not yet created renamed out, they hold
of this order (regions.md §3.4). -/
def Facts.age : Facts := fun C s => ∀ q p, C (.r q) → s.reach (.r p) → q < p

/-- `⊢ μ : Ψ` of core.md §3.6. -/
def StoreTyM (mode : TypingMode) (Ψ : List Ty) (μ : List Tm) : Prop :=
  μ.length = Ψ.length ∧
    ∀ (i : Nat) v t, μ[i]? = some v → Ψ[i]? = some t →
      Value v ∧ TypM S mode Ψ Facts.age [] v (.ex t .pure)

abbrev StoreTy := StoreTyM S .captures

/-- The root environment `Σ₀` of core.md §2.6 and the bodies of the globals of λFlick⁺. -/
structure Roots where
  svc : Name → Option Tm
  glob : Nat → Option Tm

/-- `Σ₀` has closed values of `S.Impl`. The body of a global is closed source code,
typed at the scheme of the global; each declared global has a body. -/
structure RootTyM (mode : TypingMode) (root : Roots) : Prop where
  svc : ∀ N v, root.svc N = some v → Value v ∧ TypM S mode [] Facts.none [] v (.ex (S.impl N) .pure)
  glob : ∀ g v, root.glob g = some v → (∀ x, ¬ v.Has x) ∧
    ∃ s, S.glob g = some s ∧ TypM S mode [] s.cs [] v (.ex s.t .pure)
  globs : ∀ g s, S.glob g = some s → (root.glob g).isSome
  /-- A root is source code: it names no run-time region. -/
  svcBelow : ∀ N v, root.svc N = some v → ∀ p, ¬ v.Has (.rgn p)

abbrev RootTy := RootTyM S .captures

/-- An ordinary constructor at `T us`. -/
theorem Typ.ctor_plain {Ψ Θ Γ K k ts T₀ us T φ} (hK : S.ctor K = some (k, ts, T₀))
    (hres : S.res K = idTs k) (hk : us.length = k) (hT : Le (Sig.ctorTy ts T₀ us) T) :
    Typ S Ψ Θ Γ (.ctor K) (.ex T φ) :=
  .ctor hK hk (S.holds_plain hK hres hk) (by rw [Sig.ctorTyR_plain hres hk]; exact hT)

end Flick
