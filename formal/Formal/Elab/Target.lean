import Formal.Elab.Source
import Formal.Elab.Hyp

/-!
# Elaboration: the target λFlick⁺ as an interface (elaboration.md §0.1, §2.1, §2.2)

The target is abstract: `Target` gives its typing `⊢⁺` of elaborated terms and its run as
fields, and the laws T6 needs as fields too. `Elab/Core*.lean` instantiate it with λFlick⁺ (`docs/formal/elaboration.md#dictionaries-need-polymorphic-fields-and-higher-rank-parameters`).

Types of elaborated terms: monotypes `⟦τ⟧ = τ`, arrows `a →φ^q b` and dictionary types `D_Q`.
A context entry is `∀ᾱ. t`; a record of `D_Q` with `∀`-fields shows only through its laws:
projecting a method instantiates the field's `∀`, projecting a slot gives the superclass.
`⌈D_Q⌉ = ∅`: a dictionary holds nothing (§0.1), so the arrows of methods, instances and
`struct_C` have the qualifier `cnil`; so do those of the constants of Γ₀, top-level values.

The run is about closed typed terms: `load_rel` takes the typings of both terms, and
`Target.sim` and `T6b_target` the typing of each filled skeleton.
-/

namespace Flick.Elab

/-- Types of elaborated terms. -/
inductive TTy where
  | mono (t : Ty)
  | arr (φ : Stage) (a b : TTy) (q : Ty)
  | dict (q : Cn)

def TTy.subst (σ : Nat → Ty) : TTy → TTy
  | .mono t => .mono (t.subst σ)
  | .arr φ a b q => .arr φ (a.subst σ) (b.subst σ) (q.subst σ)
  | .dict q => .dict (q.subst σ)

/-- `⌈t⌉`, with `⌈D_Q⌉ = ∅`. -/
def TTy.reach : TTy → Nat → Prop
  | .mono t, n => t.reach n
  | .arr _ a b q, n => a.reach n ∨ b.reach n ∨ q.reach n
  | .dict _, _ => False

/-- `D_{Q₁} →P^q … →P^q D_{Qₗ} →P^q t`. -/
def dicts (q : Ty) (qs : List Cn) (t : TTy) : TTy :=
  qs.foldr (fun c t => .arr .P (.dict c) t q) t

/-- `∀ᾱ. t` with `k` binders. -/
structure TSch where
  k : Nat
  ty : TTy

def TSch.subst (σ : Nat → Ty) (s : TSch) : TSch := ⟨s.k, s.ty.subst (lift s.k σ)⟩

/-- `⌈s⌉` below its binders. -/
def TSch.reach (s : TSch) (n : Nat) : Prop := s.ty.reach (n + s.k)

/-- `⟦∀ᾱ. Q̄ ⇒ τ⟧_q = ∀ᾱ. D_Q̄ →P^q τ` (`docs/formal/elaboration.md#the-dictionary-function-of-let-sig-dict-holds-what-the-right-hand-side-holds`). -/
def Scheme.tr (s : Scheme) : TSch := ⟨s.k, dicts s.q s.ctx (.mono s.ty)⟩

/-- What `e` holds in `Γ`; dictionary variables hold nothing. -/
def heldT (Γ : List TSch) (e : Tm) (n : Nat) : Prop :=
  ∃ x s, e.Free x ∧ Γ[x]? = some s ∧ s.reach n

/-- Reflexive transitive closure. -/
inductive Star {α : Type} (R : α → α → Prop) : α → α → Prop
  | refl {a} : Star R a a
  | step {a b c} : R a b → Star R b c → Star R a c

/-- Two terms that differ only by `≈` in dictionary positions. -/
inductive TmEqv (P : Prog) : Tm → Tm → Prop
  | var {x} : TmEqv P (.var x) (.var x)
  | hole {i} : TmEqv P (.hole i) (.hole i)
  | dict {d d'} : Eqv P d d' → TmEqv P (.dict d) (.dict d')
  | meth {e e' m} : TmEqv P e e' → TmEqv P (.meth e m) (.meth e' m)
  | app {f f' a a'} : TmEqv P f f' → TmEqv P a a' → TmEqv P (.app f a) (.app f' a')
  | lam {b b'} : TmEqv P b b' → TmEqv P (.lam b) (.lam b')
  | let_ {e e' b b'} : TmEqv P e e' → TmEqv P b b' → TmEqv P (.let_ e b) (.let_ e' b')
  | lamD {l b b'} : TmEqv P b b' → TmEqv P (.lamD l b) (.lamD l b')
  | const {c} : TmEqv P (.const c) (.const c)
  | ite {c c' a a' b b'} : TmEqv P c c' → TmEqv P a a' → TmEqv P b b' →
      TmEqv P (.ite c a b) (.ite c' a' b')
  | caseOpt {e e' n n' s s'} : TmEqv P e e' → TmEqv P n n' → TmEqv P s s' →
      TmEqv P (.caseOpt e n s) (.caseOpt e' n' s')

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
  /-- (App); a pure arrow applies at any stage -/
  app : ∀ {Γ Δ f x a b q ψ φ}, Has Γ Δ f (.mono (.arr ψ a b q)) φ → Has Γ Δ x (.mono a) φ →
    (ψ = .P ∨ ψ = φ) → Has Γ Δ (.app f x) (.mono b) φ
  ite : ∀ {Γ Δ c a b t φ}, Has Γ Δ c (.mono boolTy) φ → Has Γ Δ a (.mono t) φ →
    Has Γ Δ b (.mono t) φ → Has Γ Δ (.ite c a b) (.mono t) φ
  caseOpt : ∀ {Γ Δ e n s a t φ}, Has Γ Δ e (.mono (optTy a)) φ → Has Γ Δ n (.mono t) φ →
    Has (⟨0, .mono a⟩ :: Γ) Δ s (.mono t) φ → Has Γ Δ (.caseOpt e n s) (.mono t) φ
  /-- (Lam) of `regions.md`: the arrow shows what the closure holds -/
  lam : ∀ {Γ Δ b a t q φ ψ}, Has (⟨0, .mono a⟩ :: Γ) Δ b (.mono t) φ →
    (∀ n, heldT Γ (.lam b) n → (Ty.arr φ a t q).reach n) →
    Has Γ Δ (.lam b) (.mono (.arr φ a t q)) ψ
  /-- (Let-Sig) of λFlick⁺; `k = 0` is (Let-Mono) -/
  let_ : ∀ {Γ Δ e b k t u φ₁ φ}, Has (Γ.map (·.subst (up k))) (Δ.map (·.subst (up k))) e t φ₁ →
    (k ≠ 0 → φ₁ = .P) → (φ₁ = .P ∨ φ₁ = φ) → Has (⟨k, t⟩ :: Γ) Δ b u φ →
    Has Γ Δ (.let_ e b) u φ
  /-- the abstraction over dictionaries: a pure function of a pure body, whose arrows show
  what the body holds (`docs/formal/elaboration.md#the-dictionary-function-of-let-sig-dict-holds-what-the-right-hand-side-holds`) -/
  lamD : ∀ {Γ Δ qs e t q φ}, Has Γ (qs ++ Δ) e t φ → (qs ≠ [] → φ = .P) →
    (qs ≠ [] → ∀ n, heldT Γ e n → t.reach n ∨ q.reach n) →
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
  typed terms that differ by `≈` in dictionary positions, and is a weak simulation that keeps
  results. The typings are there because only typed dictionaries evaluate (`CoreDictRun.lean`). -/
  Rel : St → St → Prop
  load_rel : ∀ {e e' t φ}, TmEqv P e e' → Has [] [] e t φ → Has [] [] e' t φ →
    Rel (load e) (load e')
  step_rel : ∀ {a a' b}, Rel a b → Step a a' → ∃ b', Star Step b b' ∧ Rel a' b'
  final_rel : ∀ {a b o}, Rel a b → obs a = some o → ∃ b', Star Step b b' ∧ obs b' = some o

end Flick.Elab
