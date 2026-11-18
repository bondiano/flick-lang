import Formal.EvalCtx
import Formal.RuleRows
import Formal.Live.Mode
import Formal.Judgment

/-!
# Typing rules of λFlick (core.md §2.3, §3.6; regions.md §2.3–§2.5, §3.4)

`Γ;Θ;Ψ ⊢ e : τ ! φ`. Every rule types its premises at the stage `φ` of the
conclusion, and subsumption (Sub) is folded into the rules whose result type
may be an arrow or an `Eff` (`LeLive … T`); `Typ.sub` proves (Sub) admissible.

`Γ` holds schemes; `Θ` the facts `Outlives C ϱ`. (Let-Gen) types its right-hand
side with `k` fresh variables, the indices `0 … k-1`, with `Γ`, `Θ` and `Ψ`
shifted past them, so that `ᾱ ∉ ftv(Γ) ∪ ftv(Ψ)` holds by construction; (Scope)
does the same with one variable, its region `s`, which is then not in `fr(Γ)`,
`fr(τ)`, `fr(r)` or `fr(ε)`.
-/

namespace Flick

variable (S : Sig)

inductive Typ : List Ty → Facts → List Scheme → Tm → J → Prop
  | var {ts : List Ty} : Γ[i]? = some σ → ts.length = σ.k → σ.Holds Θ ts → LeLiveI S (σ.inst ts) T →
      Typ Ψ Θ Γ (.var i) (.ex T φ)
  /-- A value of a polymorphic type, typed under `k` fresh variables as in (Let-Gen). -/
  | allI : Value e → Typ (Ψ.map (Ty.up k)) (Θ.substT (shiftV k)) (Γ.map (Scheme.up k)) e (.ex t .pure) →
      Typ Ψ Θ Γ e (.ex (.all k t) φ)
  /-- A variable of a scheme without facts, as a value of its polymorphic type. -/
  | allVar : Γ[i]? = some σ → 0 < σ.k → (∀ C s, ¬ σ.cs C s) → Typ Ψ Θ Γ (.var i) (.ex (.all σ.k σ.t) φ)
  | lam : Typ Ψ Θ (.mono a :: Γ) b (.ex t (Stage.use ψ.asStage C)) → b.NoIns →
      LeLive S (.arrow a ψ t C) T →
      Typ Ψ Θ Γ (.lam b) (.ex T φ)
  | app : Typ Ψ Θ Γ f (.ex (.arrow a ψ t C) φ) → Typ Ψ Θ Γ e (.ex a φ) → (Stage.use ψ.asStage C).le φ →
      LeLive S t T →
      Typ Ψ Θ Γ (.app f e) (.ex T φ)
  | let_ : Typ Ψ Θ Γ e (.ex a φ) → Typ Ψ Θ (.mono a :: Γ) b (.ex t φ) →
      Typ Ψ Θ Γ (.let_ e b) (.ex t φ)
  | letGen : Typ (Ψ.map (Ty.up k)) (Facts.union (Θ.substT (shiftV k)) Φ) (Γ.map (Scheme.up k)) e
        (.ex a .pure) →
      Typ Ψ Θ (⟨k, Φ, a⟩ :: Γ) b (.ex t φ) → Typ Ψ Θ Γ (.let_ e b) (.ex t φ)
  | letrec : Typ Ψ Θ (.mono a :: .mono (.arrow a ψ r C) :: Γ) f (.ex r (Stage.use ψ.asStage C)) →
      f.NoIns →
      Typ Ψ Θ (.mono (.arrow a ψ r C) :: Γ) b (.ex t φ) → Typ Ψ Θ Γ (.letrec f b) (.ex t φ)
  | fix : Typ Ψ Θ (.mono a :: .mono (.arrow a ψ r C) :: Γ) f (.ex r (Stage.use ψ.asStage C)) →
      f.NoIns →
      LeLive S (.arrow a ψ r C) T →
      Typ Ψ Θ Γ (.fix f) (.ex T φ)
  | lit : Typ Ψ Θ Γ (.lit l) (.ex (.base l.base) φ)
  | loc : Ψ[l]? = some t → Typ Ψ Θ Γ (.loc l) (.ex (.ref t) φ)
  | const : c.ok Θ ι → LeLive S (c.ty ι) T → Typ Ψ Θ Γ (.const c) (.ex T φ)
  | ctor {ts us : List Ty} : S.ctor K = some (k, ts, T₀) → us.length = k →
      Sig.Holds ts T₀ (S.res K) us → LeLive S (Sig.ctorTyR ts T₀ (S.res K) us) T →
      Typ Ψ Θ Γ (.ctor K) (.ex T φ)
  | rnil : Typ Ψ Θ Γ .rnil (.ex .rnil φ)
  | rcons : R.IsRec → l ∉ R.labels → Typ Ψ Θ Γ e (.ex t φ) → Typ Ψ Θ Γ r (.ex R φ) →
      Typ Ψ Θ Γ (.rcons l e r) (.ex (.rcons l t R) φ)
  | proj : Typ Ψ Θ Γ e (.ex R φ) → R.field l = some t → LeLive S t T → Typ Ψ Θ Γ (.proj e l) (.ex T φ)
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
      (∀ θ, Fits θ b → Ty.Equiv ((Ty.tcon T₀ (S.res K)).substT θ) ((t.up k).substT θ) →
        PatsTy S qs (ts.map (Ty.substT θ)) (Δ θ)) →
      (∀ θ, Fits θ b → Ty.Equiv ((Ty.tcon T₀ (S.res K)).substT θ) ((t.up k).substT θ) →
        Typ ((Ψ.map (Ty.up k)).map (Ty.substT θ)) ((Θ.substT (shiftV k)).substT θ)
          ((Δ θ).map .mono ++ (Γ.map (Scheme.up k)).map (Scheme.substT θ)) b
          (.ex ((t'.up k).substT θ) ((φ.substT (shiftV k)).substT θ))) →
      Typ Ψ Θ Γ r (.alts t t' φ) → Typ Ψ Θ Γ (.acons (Pat.spine K qs) b r) (.alts t t' φ)
  | todo : Typ Ψ Θ Γ .todo (.ex t φ)
  | task : Typ Ψ Θ Γ e (.ex t ψ) → ψ.le (Stage.use (.rt r.asRow ε.asRow) C) → e.NoIns →
      LeLive S (.eff r ε t C) T →
      Typ Ψ Θ Γ (.task e) (.ex T φ)
  | await : Typ Ψ Θ Γ e (.ex (.eff r ε t C) φ) → (Stage.use (.rt r.asRow ε.asRow) C).le φ →
      LeLive S t T → Typ Ψ Θ Γ (.await e) (.ex T φ)
  /-- (T-Lazy) of lazy.md §2: the initializer is typed as the body of a `task`; making the
  cell is a runtime step, as `Ref.make`. (T-Force): forcing runs it as `await` runs a task. -/
  | lazy : Typ Ψ Θ Γ e (.ex t ψ) → ψ.le (Stage.use (.rt r.asRow ε.asRow) C) → e.NoIns →
      (Stage.rt Row.none Row.none).le φ → Typ Ψ Θ Γ (.lazy e) (.ex (.lazy r ε t C) φ)
  | force : Typ Ψ Θ Γ e (.ex (.lazy r ε t C) φ) → (Stage.use (.rt r.asRow ε.asRow) C).le φ →
      LeLive S t T → Typ Ψ Θ Γ (.force e) (.ex T φ)
  | raise : Typ Ψ Θ Γ e (.ex (.errors ε) φ) → (Stage.rt Row.none ε.asRow).le φ →
      Typ Ψ Θ Γ (.raise e) (.ex t φ)
  | raiseAdt : Typ Ψ Θ Γ e (.ex U φ) → U.hd = .adt E → (Stage.rt Row.none (Row.err E U)).le φ →
      Typ Ψ Θ Γ (.raise e) (.ex t φ)
  | errVal : Value e → Typ Ψ Θ Γ e (.ex U φ) → U.hd = .adt E → Row.Sub (Row.err E U) ε.asRow →
      Typ Ψ Θ Γ e (.ex (.errors ε) φ)
  /-- An arm that catches a name catches it at every argument (`Row.nameClose`). -/
  | tryR : Typ Ψ Θ Γ e (.ex t (.rt r ε₀.asRow)) → Typ Ψ Θ Γ A (.halts ε₀ H t φ) →
      (Stage.rt r ε').le φ → Row.Sub ε₀.asRow (Row.union H.nameClose (ε'.back H)) →
      Typ Ψ Θ Γ (.try_ e A) (.ex t φ)
  | tryP : Typ Ψ Θ Γ e (.ex t .pure) → Typ Ψ Θ Γ A (.halts .cnil H t φ) →
      Typ Ψ Θ Γ (.try_ e A) (.ex t φ)
  /-- The arms on a raised value `v` of the error type `U = E ū`, one of `ε₀`. -/
  | handle : Value v → Typ Ψ Θ Γ v (.ex U .pure) → U.hd = .adt E → Row.Sub (Row.err E U) ε₀.asRow →
      Typ Ψ Θ Γ A (.halts ε₀ H t φ) → HCov H ε' E U →
      (∀ x, ε₀.asRow x → (∃ N c, x = .nm N c) ∨ Row.union H ε'.modNames x) →
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
      (Stage.rt r ε).le φ → LeLive S t T → Typ Ψ Θ Γ (.scope e) (.ex T φ)
  | inside : (∀ C s, F C s → s = .rg p ∧ ∀ q, C (.r q) → q < p) →
      Typ Ψ (Facts.union Θ F) Γ e (.ex t φ') →
      φ'.le (.rt (Row.union (Row.scope (.rg p)) (Row.union r (Row.disch (.r p) r))) ε) →
      ¬ t.reach (.r p) → ¬ ε.reach (.r p) → (Stage.rt r ε).le φ → LeLive S t T →
      Typ Ψ Θ Γ (.inside p e) (.ex T φ)
  | cap : Typ Ψ Θ Γ (.cap p) (.ex (.hnd (.rg p)) φ)
  | own : Typ Ψ Θ Γ (.own p) (.ex (.owner (.rg p)) φ)
  | drec {us : List Ty} {fs : List (Label × Scheme)} : S.dict c = some (k, fs) → us.length = k →
      Typ Ψ Θ Γ r (.dfs (fs.map fun f => (f.1, f.2.substT (instT us)))) → r.NoIns →
      Typ Ψ Θ Γ (.drec c r) (.ex (.dict c (Ty.spine us)) φ)
  | dnil : Typ Ψ Θ Γ .rnil (.dfs [])
  | dcons {s : Scheme} {fs : List (Label × Scheme)} : Typ (Ψ.map (Ty.up s.k)) (Facts.union (Θ.substT (shiftV s.k)) s.cs) (Γ.map (Scheme.up s.k)) v
        (.ex s.t .pure) →
      Typ Ψ Θ Γ r (.dfs fs) → Typ Ψ Θ Γ (.rcons l v r) (.dfs ((l, s) :: fs))
  | dproj {ts us : List Ty} {fs : List (Label × Scheme)} {s : Scheme} : Typ Ψ Θ Γ e (.ex (.dict c (Ty.spine us)) φ) → S.dict c = some (k, fs) →
      us.length = k → fs.lookup l = some s → ts.length = s.k → (s.substT (instT us)).Holds Θ ts →
      LeLive S ((s.substT (instT us)).inst ts) T → Typ Ψ Θ Γ (.proj e l) (.ex T φ)
  | glob {ts : List Ty} : S.glob g = some σ → ts.length = σ.k → σ.Holds Θ ts → LeLive S (σ.inst ts) T →
      Typ Ψ Θ Γ (.glob g) (.ex T φ)
  /-- An operation of a declared effect at the arguments `us`: its value needs the effect at
  them; an `abort` operation never returns, so its result has any type. -/
  | perform {us : List Ty} : us.length = S.arity E → S.op E o = some (c, a, b) →
      LeLive S (.arrow (a.substT (instT us)) (.rt (.rnm E (Ty.tcon E us)) .cnil)
        (if c = .abort then t else b.substT (instT us)) Caps.none) T →
      Typ Ψ Θ Γ (.const (.perform E o c)) (.ex T φ)
  /-- The arms of a handler, `x` the operation's argument and, for `control`, `k` its
  `resume`, which runs the rest of the body under the handler and holds its region. The types
  of the operations are those at the arguments `us` of the arms. -/
  | hanil {us : List Ty} : Typ Ψ Θ Γ .anil (.harms E us t φ s)
  | hatail {us : List Ty} : us.length = S.arity E → S.op E o = some (.tail, a, b) →
      Typ Ψ Θ (.mono (a.substT (instT us)) :: Γ) e (.ex (b.substT (instT us)) φ.asStage) →
      Typ Ψ Θ Γ r (.harms E us t φ s) →
      Typ Ψ Θ Γ (.acons (Pat.spine o [.var]) e r) (.harms E us t φ s)
  | haabort {us : List Ty} : us.length = S.arity E → S.op E o = some (.abort, a, b) →
      Typ Ψ Θ (.mono (a.substT (instT us)) :: Γ) e (.ex t φ.asStage) →
      Typ Ψ Θ Γ r (.harms E us t φ s) →
      Typ Ψ Θ Γ (.acons (Pat.spine o [.var]) e r) (.harms E us t φ s)
  | hactl {us : List Ty} : us.length = S.arity E → S.op E o = some (.control, a, b) →
      -- The handler's own region is live while its control arm calls resume.
      Typ Ψ Θ (.mono (a.substT (instT us)) :: .mono (.arrow (b.substT (instT us)) φ t s.reach) :: Γ)
        e (.ex t (Stage.use φ.asStage s.reach)) →
      Typ Ψ Θ Γ r (.harms E us t φ s) →
      Typ Ψ Θ Γ (.acons (Pat.spine o [.var, .var]) e r) (.harms E us t φ s)
  /-- (Handle): the body may perform `E` at the arguments the handler answers (`Sig.answers`,
  `Row.hBody`); the arms, at the arguments `us`, run at the handler's stage, `resume`
  over a region variable of the handler, as the region of a `scope`; `rf` makes the
  handler's value of the body's. Without an arm for every operation, the body's other
  operations of `E` go on to the handler around. -/
  | hdl {al rf : Tm} {us : List Ty} : us.length = S.arity E →
      Typ (Ψ.map (Ty.up 1)) (Θ.substT (shiftV 1)) (Γ.map (Scheme.up 1)) al
        (.harms E (us.map (Ty.up 1)) (t'.up 1) ((Ty.rt r ε).substT (shiftV 1)) (.tvar 0)) →
      al.NoIns → Typ Ψ Θ Γ rf (.ex (.arrow t ψ t' C) (.rt r.asRow ε.asRow)) →
      (Stage.use ψ.asStage C).le (.rt r.asRow ε.asRow) →
      Typ Ψ Θ Γ e (.ex t φ₀) → φ₀.le (.rt (Row.hBody E (S.answers E us) r.asRow) ε.asRow) →
      φ₀.ObOut (Row.single E) →
      S.Handles E us al r.asRow → (Stage.rt r.asRow ε.asRow).le φ → LeLive S t' T →
      Typ Ψ Θ Γ (.hdl E al rf e) (.ex T φ)
  /-- The run-time forms of a handler, whose region is `p`; the cell `ℓ` holds the type and
  the stage of its value, and the arguments `us` of its arms as `E us`. -/
  | hwith {al rf : Tm} {us : List Ty} : us.length = S.arity E →
      Ψ[ℓ]? = some (.arrow (Ty.tcon E us) (.rt r ε) t' Caps.none) →
      Typ Ψ Θ Γ al (.harms E us t' (.rt r ε) (.rg p)) →
      Typ Ψ Θ Γ rf (.ex (.arrow t ψ t' C) (.rt r.asRow ε.asRow)) →
      (Stage.use ψ.asStage C).le (.rt r.asRow ε.asRow) →
      Typ Ψ Θ Γ e (.ex t φ₀) → φ₀.le (.rt (Row.hP Θ p E (S.answers E us) r.asRow) ε.asRow) →
      S.Handles E us al r.asRow → (Stage.rt r.asRow ε.asRow).le φ → LeLive S t' T →
      φ.svc (.atm (.r p)) →
      Typ Ψ Θ Γ (.hwith p ℓ E al rf e) (.ex T φ)
  /-- While an arm runs, the body is parked where it performed the operation `o`, which no frame
  of it answers: it is typed with the operation at the hole, the argument left out. A `tail` arm gives that
  operation its value, a `control` arm the handler its value, with `resume` the cell `κ`. -/
  | hctl {Ei : List Frame} {al rf : Tm} {tl : Bool} {us : List Ty} : us.length = S.arity E →
      Ψ[ℓ]? = some (.arrow (Ty.tcon E us) (.rt r ε) t' Caps.none) →
      Typ Ψ Θ Γ al (.harms E us t' (.rt r ε) (.rg p)) →
      Typ Ψ Θ Γ rf (.ex (.arrow t ψ t' C) (.rt r.asRow ε.asRow)) →
      (Stage.use ψ.asStage C).le (.rt r.asRow ε.asRow) →
      S.op E o = some (c, a₀, b₀) → c ≠ .abort → (tl = true ↔ c = .tail) →
      OkE Ei → k = plug Ei (.hole p) → (∀ G ∈ Ei, G.handles E o c = none) →
      Typ Ψ Θ [] (plug Ei (.app (.const (.perform E o c)) .todo)) (.ex t φ₀) →
      φ₀.le (.rt (Row.hP Θ p E (S.answers E us) r.asRow) ε.asRow) →
      Typ Ψ Θ Γ a (.ex (if tl then b₀.substT (instT us) else t')
        (Stage.use (.rt r.asRow ε.asRow) (if tl then Caps.none else (Ty.rg p).reach))) →
      (tl = false → ∃ C', Ψ[κ]? = some (.arrow (b₀.substT (instT us)) (.rt r ε) t' C') ∧ C' (.r p)) →
      S.Handles E us al r.asRow → (Stage.rt r.asRow ε.asRow).le φ → LeLive S t' T →
      φ.svc (.atm (.r p)) →
      Typ Ψ Θ Γ (.hctl p ℓ κ tl E al rf a k) (.ex T φ)
  /-- While the resumed body runs, the arm is parked where it called `resume`, the
  argument left out. -/
  | hres {A : List Frame} {u : Ty} : Ψ[ℓ]? = some (.arrow u (.rt r ε) t' Caps.none) →
      Typ Ψ Θ Γ b (.ex t' (Stage.use (.rt r.asRow ε.asRow) (Ty.rg p).reach)) →
      OkE A → a = plug A (.hole p) → Typ Ψ Θ [] (plug A (.app (.kont p κ) .todo))
        (.ex t' (Stage.use (.rt r.asRow ε.asRow) (Ty.rg p).reach)) →
      Ψ[κ]? = some (.arrow b₀ (.rt r ε) t' C') → C' (.r p) → (Stage.rt r.asRow ε.asRow).le φ →
      LeLive S t' T → Typ Ψ Θ Γ (.hres p ℓ a b) (.ex T φ)
  | hdone : Typ Ψ Θ Γ a (.ex t (Stage.use φ (Ty.rg p).reach)) →
      Typ Ψ Θ Γ (.hdone p a) (.ex t φ)
  /-- Unwinding to the handler of the cell `ℓ`, which then runs `a` with the type of its
  value; unwinding itself has any type. -/
  | unw {u : Ty} : Ψ[ℓ]? = some (.arrow u (.rt r ε) t' Caps.none) →
      Typ Ψ Θ Γ a (.ex t' (Stage.use (.rt r.asRow ε.asRow) (Ty.rg p).reach)) →
      (Stage.rt Row.none Row.none).le φ → Typ Ψ Θ Γ (.unw p ℓ a) (.ex t φ)
  /-- A handler kept around a child: `ℓ` and the arms are those of the handler it keeps. The
  child runs the arms, so its stage holds theirs; an operation the kept handler does not answer
  goes on to the handler around, or the root. -/
  | hkeep {al : Tm} {us : List Ty} : us.length = S.arity E →
      Ψ[ℓ]? = some (.arrow (Ty.tcon E us) (.rt r ε) t' Caps.none) →
      Typ Ψ Θ Γ al (.harms E us t' (.rt r ε) (.rg q)) →
      Typ Ψ Θ Γ e (.ex t φ₀) → φ₀.le (.rt (Row.hP Θ q E (S.answers E us) r₁) ε₁) →
      S.Handles E us al r₁ → S.NonTail E us r₁ → (Stage.rt r.asRow ε.asRow).le (.rt r₁ ε₁) →
      (Stage.rt r₁ ε₁).le φ → LeLive S t T → Typ Ψ Θ Γ (.hkeep q ℓ E al e) (.ex T φ)
  /-- While an arm of a kept handler runs, the body is parked where it performed the operation
  `o`, as for `hctl` of a `tail` operation. -/
  | hkarm {Ei : List Frame} {al : Tm} {us : List Ty} : us.length = S.arity E →
      Ψ[ℓ]? = some (.arrow (Ty.tcon E us) (.rt r ε) t' Caps.none) →
      Typ Ψ Θ Γ al (.harms E us t' (.rt r ε) (.rg q)) →
      S.op E o = some (.tail, a₀, b₀) → OkE Ei → k = plug Ei (.hole q) →
      (∀ G ∈ Ei, G.handles E o .tail = none) →
      Typ Ψ Θ [] (plug Ei (.app (.const (.perform E o .tail)) .todo)) (.ex t φ₀) →
      φ₀.le (.rt (Row.hP Θ q E (S.answers E us) r₁) ε₁) → S.Handles E us al r₁ →
      S.NonTail E us r₁ → Typ Ψ Θ Γ a (.ex (b₀.substT (instT us)) (.rt r.asRow ε.asRow)) → (Stage.rt r.asRow ε.asRow).le (.rt r₁ ε₁) →
      (Stage.rt r₁ ε₁).le φ → LeLive S t T → Typ Ψ Θ Γ (.hkarm q ℓ E al a k) (.ex T φ)
  /-- `resume`: the type of its cell, which holds the handler's region. -/
  | kont : Ψ[ℓ]? = some (.arrow a ψ t C) → C (.r p) → LeLive S (.arrow a ψ t C) T →
      Typ Ψ Θ Γ (.kont p ℓ) (.ex T φ)
  /-- A lazy cell, of the type `lazyCell` in the store: its location, and its frame while its
  initializer runs. -/
  | lazyLoc : Ψ[l]? = some (.lazyCell r ε t C) → Typ Ψ Θ Γ (.loc l) (.ex (.lazy r ε t C) φ)
  | settle : Ψ[l]? = some (.lazyCell r ε t C) → Typ Ψ Θ Γ e (.ex t ψ) →
      ψ.le (Stage.use (.rt r.asRow ε.asRow) C) → (Stage.use (.rt r.asRow ε.asRow) C).le φ →
      LeLive S t T →
      Typ Ψ Θ Γ (.settle l e) (.ex T φ)
  /-- Row equations are type equality even below invariant constructors. -/
  | conv : Typ Ψ Θ Γ e (.ex a φ) → Ty.Equiv a b → Typ Ψ Θ Γ e (.ex b φ)

end Flick
