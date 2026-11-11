import Formal.Elab.Source
import Formal.Elab.Hyp

/-!
# Elaboration: the target λFlick⁺ as an interface (elaboration.md §0.1, §2.1, §2.2)

The target is abstract: `Target` gives its typing `⊢⁺` of elaborated terms and its run as
fields, and the laws T6 needs as fields too. `Elab/Core*.lean` instantiate it with λFlick⁺ (`docs/formal/elaboration.md#dictionaries-need-polymorphic-fields-and-higher-rank-parameters`).

Types of elaborated terms: monotypes `⟦τ⟧ = τ`, also a parameter's `∀ᾱ. τ`, arrows `a →φ^q b` and dictionary types `D_Q`.
A context entry is `∀ᾱ. t`; a record of `D_Q` with `∀`-fields shows only through its laws:
projecting a method instantiates the field's `∀`, projecting a slot gives the superclass.
`⌈D_Q⌉ = ∅`: a dictionary holds nothing (§0.1), so the arrows of methods, instances and
`struct_C` have the qualifier `cnil`; so do those of the constants of Γ₀, top-level values.

The run is about closed typed terms: `load_rel` takes the typings of two fillings of a
skeleton and of the dictionaries in its holes, and `Target.sim` and `T6b_target` the typing of
each filled skeleton.
-/

namespace Flick.Elab

/-- Types of elaborated terms. -/
inductive TTy where
  | mono (t : Ty)
  | arr (φ : Stage) (a b : TTy) (q : Ty)
  | dict (q : Cn)

def TTy.subst (σ : Nat → Ty) : TTy → TTy
  | .mono t => .mono (t.subst σ)
  | .arr φ a b q => .arr (φ.subst σ) (a.subst σ) (b.subst σ) (q.subst σ)
  | .dict q => .dict (q.subst σ)

/-- `⌈t⌉`, with `⌈D_Q⌉ = ∅`: what `t` shows (`Ty.Shown`); an arrow does not count the rows of
its stage. -/
def TTy.reach : TTy → Nat → Prop
  | .mono t, n => t.Shown n
  | .arr _ a b q, n => a.reach n ∨ b.reach n ∨ q.Shown n
  | .dict _, _ => False

/-- `D_{Q₁} →P^q … →P^q D_{Qₗ} →P^q t`. -/
def dicts (q : Ty) (qs : List Cn) (t : TTy) : TTy :=
  qs.foldr (fun c t => .arr .P (.dict c) t q) t

/-- `∀ᾱ. t` with `k` binders. -/
structure TSch where
  k : Nat
  ty : TTy

def TSch.subst (σ : Nat → Ty) (s : TSch) : TSch := ⟨s.k, s.ty.subst (lift s.k σ)⟩

/-- `⟦∀ᾱ. Q̄ ⇒ τ⟧_q = ∀ᾱ. D_Q̄ →P^q τ` (`docs/formal/elaboration.md#the-dictionary-function-of-let-sig-dict-holds-what-the-right-hand-side-holds`). -/
def Scheme.tr (s : Scheme) : TSch := ⟨s.k, dicts s.q s.ctx (.mono s.ty)⟩

/-- Reflexive transitive closure. -/
inductive Star {α : Type} (R : α → α → Prop) : α → α → Prop
  | refl {a} : Star R a a
  | step {a b c} : R a b → Star R b c → Star R a c

/-- Fill the holes of a skeleton with `ds`. -/
def Tm.fill (ds : List Dict) : Tm → Tm
  | .var x => .var x
  | .hole i => .dict (args ds i)
  | .dict d => .dict d
  | .meth e m => .meth (e.fill ds) m
  | .app f a => .app (f.fill ds) (a.fill ds)
  | .lam b => .lam (b.fill ds)
  | .let_ e b => .let_ (e.fill ds) (b.fill ds)
  | .lamD l b => .lamD l (b.fill ds)
  | .const c => .const c
  | .ite c a b => .ite (c.fill ds) (a.fill ds) (b.fill ds)
  | .caseOpt e n s => .caseOpt (e.fill ds) (n.fill ds) (s.fill ds)
  | .con K => .con K
  | .caseK e K n m b r => .caseK (e.fill ds) K n m (b.fill ds) (r.fill ds)

/-- The context of an arm on `c` under `θ` in the target: its fields, then `Γ` past the
variables of `c`. -/
def armT (c : Ctor) (θ : Nat → Ty) (Γ : List TSch) : List TSch :=
  c.fields.map (fun t => (⟨0, .mono (t.subst θ)⟩ : TSch)) ++ (Γ.map (·.subst (up c.k))).map (·.subst θ)

/-- The target. `Has Γ Δ e t φ` is `Γ, ⟦Δ⟧ ⊢⁺ e : t ! φ`, where a premise `δ : Q` of `Δ` is
the variable `δ : D_Q`. -/
structure Target (P : Prog) where
  Has : List TSch → List Cn → Tm → TTy → Stage → Prop
  /-- (Var) with the instantiation of `∀` -/
  var : ∀ {Γ Δ x s ts φ}, Γ[x]? = some s → ts.length = s.k →
    Has Γ Δ (.var x) (s.ty.subst (inst ts)) φ
  /-- (Const) with the instantiation of `∀` -/
  const : ∀ {Γ Δ c s ts φ}, P.consts c = some s → ts.length = s.k →
    Has Γ Δ (.const c) ((dicts .cnil s.ctx (.mono s.ty)).subst (inst ts)) φ
  /-- (∀I) at a lambda -/
  allLam : ∀ {Γ Δ k b t ψ}, Has (Γ.map (·.subst (up k))) (Δ.map (·.subst (up k))) (.lam b) (.mono t) .P →
    Has Γ Δ (.lam b) (.mono (.all k t)) ψ
  /-- (∀I-Var) -/
  allVar : ∀ {Γ Δ x k t ψ}, Γ[x]? = some ⟨k, .mono t⟩ → 0 < k → Has Γ Δ (.var x) (.mono (.all k t)) ψ
  /-- (Var) at an instance of a binder -/
  varAll : ∀ {Γ Δ x k₀ t₀ ts k t us φ}, Γ[x]? = some ⟨k₀, .mono t₀⟩ → ts.length = k₀ →
    t₀.subst (inst ts) = .all k t → us.length = k →
    Has Γ Δ (.var x) (.mono (t.subst (inst us))) φ
  /-- (App); a pure arrow applies at any stage, a runtime one where its qualifier is live -/
  app : ∀ {Γ Δ f x a b q ψ φ}, Has Γ Δ f (.mono (.arr ψ a b q)) φ → Has Γ Δ x (.mono a) φ →
    ψ.Applies φ q → Has Γ Δ (.app f x) (.mono b) φ
  ite : ∀ {Γ Δ c a b t φ}, Has Γ Δ c (.mono boolTy) φ → Has Γ Δ a (.mono t) φ →
    Has Γ Δ b (.mono t) φ → Has Γ Δ (.ite c a b) (.mono t) φ
  caseOpt : ∀ {Γ Δ e n s a t φ}, Has Γ Δ e (.mono (optTy a)) φ → Has Γ Δ n (.mono t) φ →
    Has (⟨0, .mono a⟩ :: Γ) Δ s (.mono t) φ → Has Γ Δ (.caseOpt e n s) (.mono t) φ
  /-- (Con) with the instantiation of `∀`: a pure function of its dictionaries and fields -/
  con : ∀ {Γ Δ K c ts φ}, P.ctors K = some c → ts.length = c.k →
    Has Γ Δ (.con K) ((dicts .cnil c.ctx (.mono c.ty)).subst (inst ts)) φ
  /-- (Case) closed by the types `σ` and the dictionaries `ρ` of the premises `Δ`, ground
  under `σ`: the arm is typed under the most general unifier `θ` of the open problem, before
  the closing. The closing may leave a problem whose unifiers have no most general one, as
  rows and captures are sets in the core. -/
  caseK : ∀ {Γ : List TSch} {Δl Δ : List Cn} {e b r : Tm} {K n T : Nat} {us : List Ty} {t : Ty}
      {φ : Stage} {c : Ctor} {θ σ : Nat → Ty} {ρ : Nat → Dict},
    Has (Γ.map (·.subst σ)) (Δl.map (·.subst σ)) e (.mono ((Ty.tcon T us).subst σ)) (φ.subst σ) →
    P.ctors K = some c → c.T = T → us.length = c.res.length → n = c.fields.length →
    Unify (c.eqs us) θ →
    Has (armT c θ Γ) (armΔ c θ (Δl ++ Δ)) b (.mono ((t.subst (up c.k)).subst θ))
      ((φ.subst (up c.k)).subst θ) →
    Closes P Δ σ ρ → (∀ (k : Nat) (Q : Cn), Δ[k]? = some Q → (Q.subst σ).Ground) →
    (∀ k, (ρ k).Below 0) →
    Has (Γ.map (·.subst σ)) (Δl.map (·.subst σ)) r (.mono (t.subst σ)) (φ.subst σ) →
    Has (Γ.map (·.subst σ)) (Δl.map (·.subst σ))
      (.caseK e K n c.ctx.length (b.dsubst ρ (c.ctx.length + Δl.length)) r) (.mono (t.subst σ))
      (φ.subst σ)
  /-- (Case-Dead), at any instance `σ` of the matched type -/
  caseKDead : ∀ {Γ : List TSch} {Δ : List Cn} {e r : Tm} {K n m T : Nat} {us : List Ty}
      {σ : Nat → Ty} {t : Ty} {φ : Stage} {c : Ctor}, Has Γ Δ e (.mono ((Ty.tcon T us).subst σ)) φ →
    P.ctors K = some c → c.T = T → us.length = c.res.length → Clash (c.eqs us) →
    Has Γ Δ r (.mono t) φ → Has Γ Δ (.caseK e K n m .dead r) (.mono t) φ
  /-- (Lam) of the live discipline: the arrow need not show what the closure holds -/
  lam : ∀ {Γ Δ b a t q φ ψ}, Has (⟨0, .mono a⟩ :: Γ) Δ b (.mono t) φ →
    Has Γ Δ (.lam b) (.mono (.arr φ a t q)) ψ
  /-- (Let-Sig) of λFlick⁺; `k = 0` is (Let-Mono) -/
  let_ : ∀ {Γ Δ e b k t u φ₁ φ}, Has (Γ.map (·.subst (up k))) (Δ.map (·.subst (up k))) e t φ₁ →
    (k ≠ 0 → φ₁ = .P) → (φ₁ = .P ∨ φ₁ = φ) → Has (⟨k, t⟩ :: Γ) Δ b u φ →
    Has Γ Δ (.let_ e b) u φ
  /-- the abstraction over dictionaries: a pure function of a pure body -/
  lamD : ∀ {Γ Δ qs e t q φ}, Has Γ (qs ++ Δ) e t φ → (qs ≠ [] → φ = .P) →
    Has Γ Δ (.lamD qs.length e) (dicts q qs t) φ
  /-- applying a dictionary: a pure arrow, at any stage -/
  dapp : ∀ {Γ Δ f x a t q φ}, Has Γ Δ f (.arr .P a t q) φ → Has Γ Δ x a φ →
    Has Γ Δ (.app f x) t φ
  /-- a method of `D_{C ᾱθ}`: projection and instantiation of `∀β̄` -/
  meth : ∀ {Γ Δ e m C s c₀ rest θ φ}, P.methods m = some (C, s) → s.ctx = c₀ :: rest →
    Has Γ Δ e (.dict (c₀.subst θ)) φ →
    Has Γ Δ (.meth e m) (dicts .cnil (rest.map (·.subst θ)) (.mono (s.ty.subst θ))) φ
  dvar : ∀ {Γ Δ k q φ}, Δ[k]? = some q → Has Γ Δ (.dict (.var k)) (.dict q) φ
  /-- the superclass slots of `D_Q` -/
  slot : ∀ {Γ Δ d q i S φ}, Has Γ Δ (.dict d) (.dict q) φ → P.super q i = some S →
    Has Γ Δ (.dict (.slot d i)) (.dict S) φ
  dap : ∀ {Γ Δ f x a t q φ}, Has Γ Δ (.dict f) (.arr .P a t q) φ → Has Γ Δ (.dict x) a φ →
    Has Γ Δ (.dict (.ap f x)) t φ
  /-- the program's instances: `I : ∀β̄. D_ctx →P D_head`, by (Instance) of §2.5 with the
  slots of K7 -/
  inst : ∀ {Γ Δ I ins σ φ}, P.insts I = some ins →
    Has Γ Δ (.dict (.inst I))
      (dicts .cnil (ins.ctx.map (·.subst σ)) (.dict ⟨ins.cls, ins.head.map (·.subst σ)⟩)) φ
  /-- (Struct): `struct_C^τ : D_{C τ₁θ} →P … →P D_{C τθ}` at any instance `θ` of its type: the
  run erases `τ` (`docs/formal/elaboration.md#the-structural-dictionary-term-carries-its-type`, `docs/formal/elaboration.md#structural-dictionaries-were-typed-only-at-their-own-record-type`) -/
  strct : ∀ {Γ Δ C τ ts θ φ}, P.structC C = true → P.fields τ = some ts →
    Has Γ Δ (.dict (.strct C τ))
      (dicts .cnil (ts.map fun t => ⟨C, [t.subst θ]⟩) (.dict ⟨C, [τ.subst θ]⟩)) φ
  /-- (Prim), at any instance `θ` of the constraint (`docs/formal/elaboration.md#structural-dictionaries-were-typed-only-at-their-own-record-type`) -/
  prim : ∀ {Γ Δ q p θ φ}, P.primC q.cls = true → P.prims q = some p →
    Has Γ Δ (.dict (.prim p q)) (.dict (q.subst θ)) φ
  /-- States of a run, the loading of a term, steps, observable results. -/
  St : Type
  load : Tm → St
  Step : St → St → Prop
  Obs : Type
  obs : St → Option Obs
  /-- `≈`-related dictionaries stay related under steps: `Rel` relates the states of closed
  typed fillings of a skeleton by `≈` dictionaries, and is a weak simulation that keeps results.
  The dictionaries of the holes are typed, because only typed dictionaries evaluate
  (`CoreDictRun.lean`); the rest of the two terms is the same. -/
  Rel : St → St → Prop
  load_rel : ∀ {s : Tm} {ds ds' : List Dict} {t : TTy} {φ : Stage}, All2 (Eqv P) ds ds' →
    (∀ (i : Nat) (d : Dict), ds[i]? = some d → ∃ q, Has [] [] (.dict d) (.dict q) .P) →
    Has [] [] (s.fill ds) t φ → Has [] [] (s.fill ds') t φ → Rel (load (s.fill ds)) (load (s.fill ds'))
  step_rel : ∀ {a a' b}, Rel a b → Step a a' → ∃ b', Star Step b b' ∧ Rel a' b'
  final_rel : ∀ {a b o}, Rel a b → obs a = some o → ∃ b', Star Step b b' ∧ obs b' = some o

end Flick.Elab
